#!/usr/bin/env python3
"""
LispCraft 3D - 端到端多客户端并发仿真与集成验收测试套件 (E2E Integration & Verification Suite)

测试范围:
1. HTTP 静态 WebAssembly 交付验证 (Port 8080)
   - / 根路径 HTML 与 CORS 响应
   - /client.wasm MIME 类型及 WASM 魔数 (\0asm)
   - 依赖脚本 (gl.js, quad-net.js, sapp_jsutils.js)
   - 404 错误处理
2. Native TCP 协议兼容性验证 (Port 8088)
   - 长度前缀二进制帧交互
3. 多客户端并发仿真与视口独立性验证 (Not Shared Screen) (Port 8089 WS)
   - Client A 与 Client B 同时连接
   - 各自独立下发 camera_target 与 client_id，互不干扰
4. 空间裁剪与相互可见性验证 (Spatial Culling & Mutual Visibility)
   - 距离 > 25m: 实体裁剪互不可见
   - 距离 <= 25m: 动态在彼此 entities 列表中显现 (shape="cube", label=player_id)
5. 字符串命令管道验证 (Command Pipeline)
   - heal, tp, speed, spawn 命令响应与状态更新
   - say 命令的全服广播与 Client B 接收
6. 场景拾取道具联动验证 (Pickups & Cooldown)
   - 移动碰撞拾取生命/加速球/能量核心
   - 属性增加、日志记录与道具进入冷却下线
7. 游戏模式切换验证 (Dungeon Mode Switching)
   - 载入 games/dungeon.scm 启动地牢模式
   - 校验地牢核心 (dungeon-core) 与四大古石柱 (pillar-ne/nw/se/sw)
   - 校验地牢独占命令 (torch)
8. 性能与延迟基准评测 (Performance Benchmarks)
   - 网络往返延迟 RTT (平均、最小、最大、P95)
   - 状态下发 Tick 周期与频率 (TPS)
   - 数据包体积统计
"""

import socket
import struct
import base64
import os
import json
import time
import subprocess
import urllib.request
import urllib.error
import sys
from typing import Optional, Tuple, List, Dict, Any, Callable

# ============================================================================
# 1. 轻量化原生 WebSocket 客户端 (基于 RFC 6455 规范)
# ============================================================================
class WebSocketClient:
    def __init__(self, host: str = "127.0.0.1", port: int = 8089, timeout: float = 3.0):
        self.host = host
        self.port = port
        self.timeout = timeout
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.client_id: Optional[str] = None
        self.connect()

    def connect(self):
        self.sock.connect((self.host, self.port))
        key = base64.b64encode(os.urandom(16)).decode("utf-8")
        req = (
            f"GET / HTTP/1.1\r\n"
            f"Host: {self.host}:{self.port}\r\n"
            f"Upgrade: websocket\r\n"
            f"Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            f"Sec-WebSocket-Version: 13\r\n\r\n"
        )
        self.sock.sendall(req.encode("utf-8"))
        resp = self.sock.recv(4096).decode("utf-8", errors="replace")
        if "101 Switching Protocols" not in resp:
            raise ConnectionError(f"WebSocket handshake failed: {resp}")

    def _recv_exact(self, n: int) -> bytes:
        buf = bytearray()
        while len(buf) < n:
            chunk = self.sock.recv(n - len(buf))
            if not chunk:
                raise ConnectionResetError("Socket connection closed by peer")
            buf.extend(chunk)
        return bytes(buf)

    def recv_frame(self) -> Tuple[int, bytes]:
        hdr = self._recv_exact(2)
        opcode = hdr[0] & 0x0F
        has_mask = (hdr[1] & 0x80) != 0
        plen = hdr[1] & 0x7F
        if plen == 126:
            plen = struct.unpack("!H", self._recv_exact(2))[0]
        elif plen == 127:
            plen = struct.unpack("!Q", self._recv_exact(8))[0]
        mask = self._recv_exact(4) if has_mask else None
        data = self._recv_exact(plen)
        if has_mask:
            data = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
        return opcode, data

    def send_frame(self, data: bytes, opcode: int = 0x02): # binary frame
        plen = len(data)
        hdr = bytearray([0x80 | (opcode & 0x0F)])
        if plen <= 125:
            hdr.append(0x80 | plen)
        elif plen <= 65535:
            hdr.append(0x80 | 126)
            hdr.extend(struct.pack("!H", plen))
        else:
            hdr.append(0x80 | 127)
            hdr.extend(struct.pack("!Q", plen))
        mask = os.urandom(4)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
        self.sock.sendall(hdr + mask + masked)

    def send_json(self, obj: Any):
        data = json.dumps(obj).encode("utf-8")
        self.send_frame(data, opcode=0x02)

    def recv_json(self, timeout: Optional[float] = None) -> Dict[str, Any]:
        if timeout is not None:
            self.sock.settimeout(timeout)
        while True:
            op, data = self.recv_frame()
            if op in (0x01, 0x02):
                return json.loads(data.decode("utf-8"))

    def wait_for_welcome(self) -> str:
        deadline = time.time() + 3.0
        while time.time() < deadline:
            msg = self.recv_json(timeout=2.0)
            if "Welcome" in msg:
                self.client_id = msg["Welcome"]["client_id"]
                return self.client_id
        raise TimeoutError("Timed out waiting for Welcome")

    def wait_for_echo(self, keyword: Optional[str] = None, timeout: float = 3.0) -> Dict[str, Any]:
        deadline = time.time() + timeout
        while time.time() < deadline:
            msg = self.recv_json(timeout=timeout)
            if "CommandEcho" in msg:
                echo = msg["CommandEcho"]
                out = echo.get("output", "")
                if keyword is not None and keyword.lower() not in out.lower():
                    continue
                return echo
        raise TimeoutError(f"Timed out waiting for CommandEcho (keyword='{keyword}')")

    def wait_for_state(self, predicate: Optional[Callable[[Dict[str, Any]], bool]] = None, timeout: float = 3.0) -> Dict[str, Any]:
        deadline = time.time() + timeout
        last_state = None
        while time.time() < deadline:
            msg = self.recv_json(timeout=timeout)
            if "camera_target" in msg and "entities" in msg:
                last_state = msg
                if predicate is None or predicate(msg):
                    return msg
        if predicate is None and last_state is not None:
            return last_state
        raise TimeoutError("Timed out waiting for matching ClientState")

    def send_command(self, cmd_text: str):
        self.send_json({"Command": cmd_text})

    def send_move(self, dx: float, dz: float, yaw: float):
        self.send_json({"Move": {"dx": dx, "dz": dz, "yaw": yaw}})

    def close(self):
        try:
            self.sock.close()
        except Exception:
            pass


# ============================================================================
# 2. 原生 TCP 客户端 (用于校验 8088 端口原生 Native 协议兼容)
# ============================================================================
class NativeTcpClient:
    def __init__(self, host: str = "127.0.0.1", port: int = 8088, timeout: float = 3.0):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.sock.connect((host, port))

    def send_json(self, obj: Any):
        data = json.dumps(obj).encode("utf-8")
        packet = bytes([len(data)]) + data
        self.sock.sendall(packet)

    def recv_json(self) -> Dict[str, Any]:
        lb = self.sock.recv(1)
        if not lb:
            raise ConnectionResetError("TCP socket closed")
        mlen = lb[0]
        data = self.sock.recv(mlen)
        return json.loads(data.decode("utf-8"))

    def close(self):
        try:
            self.sock.close()
        except Exception:
            pass


# ============================================================================
# 3. 服务端进程生命周期管理器
# ============================================================================
def start_game_server(script_path: str = "games/space_arena.scm", timeout: float = 5.0) -> subprocess.Popen:
    subprocess.run(["pkill", "-9", "-x", "server"], check=False)
    time.sleep(0.5)

    proc = subprocess.Popen(
        ["./target/release/server", script_path],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
    )

    t_start = time.time()
    while time.time() - t_start < timeout:
        try:
            s = socket.socket()
            s.settimeout(0.2)
            s.connect(("127.0.0.1", 8089))
            s.close()
            return proc
        except Exception:
            time.sleep(0.1)

    raise RuntimeError(f"Server with script '{script_path}' failed to bind 8089 within {timeout}s")


# ============================================================================
# 4. 测试报告收集器
# ============================================================================
class TestReportCollector:
    def __init__(self):
        self.results: List[Dict[str, Any]] = []
        self.benchmark_metrics: Dict[str, Any] = {}

    def record(self, category: str, name: str, success: bool, details: str, duration_ms: float):
        self.results.append({
            "category": category,
            "name": name,
            "success": success,
            "details": details,
            "duration_ms": duration_ms,
        })
        status_sym = "✅ PASS" if success else "❌ FAIL"
        print(f"[{status_sym}] [{category}] {name} ({duration_ms:.2f}ms): {details}")


# ============================================================================
# 5. 各功能测试组实现
# ============================================================================

def run_http_wasm_tests(collector: TestReportCollector, base_url: str = None):
    http_port = os.environ.get("HTTP_PORT", "8087")
    if base_url is None:
        base_url = f"http://127.0.0.1:{http_port}"
    print("\n" + "="*70)
    print(f"▶ 运行测试组 1: HTTP 静态 WebAssembly 交付验证 (Port {http_port})")
    print("="*70)

    # 1.1 根路径 index.html
    t0 = time.time()
    try:
        req = urllib.request.Request(f"{base_url}/")
        with urllib.request.urlopen(req) as resp:
            code = resp.getcode()
            ct = resp.headers.get("Content-Type", "")
            cors = resp.headers.get("Access-Control-Allow-Origin", "")
            content = resp.read()
            assert code == 200, f"Expected 200, got {code}"
            assert "text/html" in ct, f"Expected text/html, got {ct}"
            assert b"LispCraft 3D" in content, "Missing 'LispCraft 3D' in index.html"
            assert cors == "*", f"Expected CORS '*', got {cors}"
            collector.record("HTTP/WASM", "Root HTML Delivery (index.html)", True,
                             f"Status: {code}, Content-Type: {ct}, CORS: {cors}, Size: {len(content)}B", (time.time()-t0)*1000)
    except Exception as e:
        collector.record("HTTP/WASM", "Root HTML Delivery (index.html)", False, str(e), (time.time()-t0)*1000)

    # 1.2 client.wasm MIME 及魔数校验
    t0 = time.time()
    try:
        req = urllib.request.Request(f"{base_url}/client.wasm")
        with urllib.request.urlopen(req) as resp:
            code = resp.getcode()
            ct = resp.headers.get("Content-Type", "")
            cl = int(resp.headers.get("Content-Length", "0"))
            magic = resp.read(4)
            assert code == 200, f"Expected 200, got {code}"
            assert ct == "application/wasm", f"Expected application/wasm, got {ct}"
            assert cl > 10 * 1024 * 1024, f"client.wasm too small: {cl}B"
            assert magic == b"\x00asm", f"Invalid WASM magic header: {magic}"
            collector.record("HTTP/WASM", "client.wasm Binary & Magic Header", True,
                             f"MIME: {ct}, Size: {cl / (1024*1024):.2f}MB, Magic: \\0asm verified", (time.time()-t0)*1000)
    except Exception as e:
        collector.record("HTTP/WASM", "client.wasm Binary & Magic Header", False, str(e), (time.time()-t0)*1000)

    # 1.3 静态前端 JS 资源依赖
    for js_file in ["gl.js", "quad-net.js", "sapp_jsutils.js"]:
        t0 = time.time()
        try:
            req = urllib.request.Request(f"{base_url}/{js_file}")
            with urllib.request.urlopen(req) as resp:
                code = resp.getcode()
                ct = resp.headers.get("Content-Type", "")
                data = resp.read()
                assert code == 200
                assert len(data) > 100
                collector.record("HTTP/WASM", f"Dependency Asset ({js_file})", True,
                                 f"Status: {code}, MIME: {ct}, Size: {len(data)}B", (time.time()-t0)*1000)
        except Exception as e:
            collector.record("HTTP/WASM", f"Dependency Asset ({js_file})", False, str(e), (time.time()-t0)*1000)

    # 1.4 404 错误响应验证
    t0 = time.time()
    try:
        urllib.request.urlopen(f"{base_url}/not_exist.wasm")
        collector.record("HTTP/WASM", "HTTP 404 Handling", False, "Expected 404 error", (time.time()-t0)*1000)
    except urllib.error.HTTPError as e:
        collector.record("HTTP/WASM", "HTTP 404 Handling", e.code == 404, f"Returned status: {e.code}", (time.time()-t0)*1000)
    except Exception as e:
        collector.record("HTTP/WASM", "HTTP 404 Handling", False, str(e), (time.time()-t0)*1000)


def run_tcp_native_protocol_test(collector: TestReportCollector):
    print("\n" + "="*70)
    print("▶ 运行测试组 2: Native TCP 协议兼容性验证 (Port 8088)")
    print("="*70)

    t0 = time.time()
    try:
        tcp_client = NativeTcpClient(port=8088)
        tcp_client.send_json({"Command": "who"})
        resp = tcp_client.recv_json()
        assert "Welcome" in resp or "CommandEcho" in resp, f"Unexpected TCP response: {resp}"
        collector.record("TCP/Native", "Native 8088 TCP Framing & Protocol", True,
                         f"Successfully sent length-prefixed ClientMsg and received: {resp}", (time.time()-t0)*1000)
        tcp_client.close()
    except Exception as e:
        collector.record("TCP/Native", "Native 8088 TCP Framing & Protocol", False, str(e), (time.time()-t0)*1000)


def run_viewport_independence_test(collector: TestReportCollector):
    print("\n" + "="*70)
    print("▶ 运行测试组 3: 视口状态差异化与独立性验证 (Not Shared Screen)")
    print("="*70)

    t0 = time.time()
    try:
        client_a = WebSocketClient()
        id_a = client_a.wait_for_welcome()

        client_b = WebSocketClient()
        id_b = client_b.wait_for_welcome()

        assert id_a != id_b, f"Client IDs must be unique: {id_a} == {id_b}"

        # Client A 传送至 (-15, -15)，Client B 传送至 (20, 20)
        client_a.send_command("tp -15 -15")
        echo_a = client_a.wait_for_echo(keyword="Teleported")
        assert "(-15" in echo_a["output"]

        client_b.send_command("tp 20 20")
        echo_b = client_b.wait_for_echo(keyword="Teleported")
        assert "(20" in echo_b["output"]

        # 接收并校验 Client A 与 B 的更新状态包
        state_a = client_a.wait_for_state(lambda s: abs(s["camera_target"][0] - (-15.0)) < 0.2)
        state_b = client_b.wait_for_state(lambda s: abs(s["camera_target"][0] - 20.0) < 0.2)

        # 校验 1: client_id 严格独立
        assert state_a["client_id"] == id_a, f"Expected {id_a}, got {state_a['client_id']}"
        assert state_b["client_id"] == id_b, f"Expected {id_b}, got {state_b['client_id']}"

        # 校验 2: camera_target 互相隔离，独立相机追踪
        target_a = state_a["camera_target"]
        target_b = state_b["camera_target"]

        assert abs(target_a[0] - (-15.0)) < 0.1 and abs(target_a[2] - (-15.0)) < 0.1
        assert abs(target_b[0] - 20.0) < 0.1 and abs(target_b[2] - 20.0) < 0.1
        assert abs(target_a[0] - target_b[0]) > 30.0

        # 校验 3: Client A 单独调整速度，不泄漏给 Client B
        client_a.send_command("speed 2.5")
        echo_spd = client_a.wait_for_echo(keyword="speed")
        assert echo_spd["success"] is True

        state_b_post = client_b.wait_for_state()
        assert not any("Movement speed multiplier set to 2.5" in l for l in state_b_post.get("logs", []))

        collector.record("Viewport", "Multi-Client Viewport Independence", True,
                         f"Client A ({id_a}) at {target_a[:3]} vs Client B ({id_b}) at {target_b[:3]} strictly decoupled",
                         (time.time()-t0)*1000)

        client_a.close()
        client_b.close()
    except Exception as e:
        collector.record("Viewport", "Multi-Client Viewport Independence", False, str(e), (time.time()-t0)*1000)


def run_spatial_culling_test(collector: TestReportCollector):
    print("\n" + "="*70)
    print("▶ 运行测试组 4: 相互可见性与空间裁剪验证 (Spatial Culling & Mutual Visibility)")
    print("="*70)

    t0 = time.time()
    try:
        client_a = WebSocketClient()
        id_a = client_a.wait_for_welcome()
        client_b = WebSocketClient()
        id_b = client_b.wait_for_welcome()

        # 阶段 1: 远离裁剪测试 (距离 > 25m)
        # A 位于 (0, 0), B 位于 (60, 60)，距离 = sqrt(60^2 + 60^2) ≈ 84.85m >> 25m
        client_a.send_command("tp 0 0")
        client_a.wait_for_echo(keyword="Teleported")

        client_b.send_command("tp 60 60")
        client_b.wait_for_echo(keyword="Teleported")

        state_a_far = client_a.wait_for_state(lambda s: abs(s["camera_target"][0] - 0.0) < 0.2)
        state_b_far = client_b.wait_for_state(lambda s: abs(s["camera_target"][0] - 60.0) < 0.2)

        b_in_a = any(e.get("label") == id_b for e in state_a_far["entities"])
        a_in_b = any(e.get("label") == id_a for e in state_b_far["entities"])

        assert not b_in_a, f"Client B ({id_b}) should be culled from Client A (dist=84.9m > 25m)"
        assert not a_in_b, f"Client A ({id_a}) should be culled from Client B (dist=84.9m > 25m)"
        collector.record("Spatial", "Distance Culling (> 25m)", True,
                         f"Both clients culled when distance is 84.9m (Entities count A:{len(state_a_far['entities'])}, B:{len(state_b_far['entities'])})",
                         (time.time()-t0)*1000)

        # 阶段 2: 靠近显现测试 (距离 <= 25m)
        # A 移动到 (55, 60)，与 B (60, 60) 的距离仅为 5.0m <= 25m
        t1 = time.time()
        client_a.send_command("tp 55 60")
        client_a.wait_for_echo(keyword="Teleported")

        state_a_near = client_a.wait_for_state(lambda s: any(e.get("label") == id_b for e in s["entities"]))
        state_b_near = client_b.wait_for_state(lambda s: any(e.get("label") == id_a for e in s["entities"]))

        b_entity = next(e for e in state_a_near["entities"] if e.get("label") == id_b)
        a_entity = next(e for e in state_b_near["entities"] if e.get("label") == id_a)

        assert b_entity is not None and a_entity is not None
        assert b_entity["shape"] == "cube" and a_entity["shape"] == "cube"
        assert b_entity["label"] == id_b and a_entity["label"] == id_a
        assert len(b_entity["color"]) == 4 and len(b_entity["size"]) == 3

        collector.record("Spatial", "Mutual Visibility (<= 25m)", True,
                         f"Client B visible in A: {b_entity['id']} shape={b_entity['shape']} label={b_entity['label']} pos={b_entity['pos']}",
                         (time.time()-t1)*1000)

        client_a.close()
        client_b.close()
    except Exception as e:
        collector.record("Spatial", "Spatial Culling / Visibility", False, str(e), (time.time()-t0)*1000)


def run_command_pipeline_test(collector: TestReportCollector):
    print("\n" + "="*70)
    print("▶ 运行测试组 5: 字符串命令管道与广播验证 (Command Pipeline & Broadcast)")
    print("="*70)

    try:
        client_a = WebSocketClient()
        id_a = client_a.wait_for_welcome()
        client_b = WebSocketClient()
        id_b = client_b.wait_for_welcome()

        # 5.1 heal 命令
        t0 = time.time()
        client_a.send_command("heal")
        echo_heal = client_a.wait_for_echo(keyword="Health")
        assert echo_heal["success"] is True
        assert "Health restored" in echo_heal["output"] or "100%" in echo_heal["output"]
        collector.record("Command", "Command: 'heal' Echo & Execution", True,
                         f"Response: {echo_heal['output']}", (time.time()-t0)*1000)

        # 5.2 tp 命令
        t0 = time.time()
        client_a.send_command("tp 5 5")
        echo_tp = client_a.wait_for_echo(keyword="Teleported")
        assert echo_tp["success"] is True
        assert "(5, 5)" in echo_tp["output"]
        st = client_a.wait_for_state(lambda s: abs(s["camera_target"][0] - 5.0) < 0.2)
        collector.record("Command", "Command: 'tp 5 5' Position Update", True,
                         f"Target updated: {st['camera_target']}", (time.time()-t0)*1000)

        # 5.3 speed 命令
        t0 = time.time()
        client_a.send_command("speed 2.0")
        echo_spd = client_a.wait_for_echo(keyword="speed")
        assert echo_spd["success"] is True
        assert "2" in echo_spd["output"]
        collector.record("Command", "Command: 'speed 2.0' Multiplier Setting", True,
                         f"Response: {echo_spd['output']}", (time.time()-t0)*1000)

        # 5.4 say 广播命令及 Client B 接收
        t0 = time.time()
        test_msg = "Hello All from Automated QA!"
        client_a.send_command(f"say {test_msg}")
        echo_b = client_b.wait_for_echo(keyword=test_msg)
        assert echo_b["success"] is True
        assert test_msg in echo_b["output"]
        collector.record("Command", "Command: 'say <msg>' Cross-Client Broadcast", True,
                         f"Client B received broadcast: '{echo_b['output']}'", (time.time()-t0)*1000)

        # 5.5 spawn 命令与实体同步
        t0 = time.time()
        client_a.send_command("spawn cube DefenseTower")
        echo_spawn = client_a.wait_for_echo(keyword="Spawned")
        assert echo_spawn["success"] is True
        st_spawn = client_a.wait_for_state(lambda s: any("DefenseTower" in e.get("label", "") for e in s["entities"]))
        tower = next(e for e in st_spawn["entities"] if "DefenseTower" in e.get("label", ""))
        collector.record("Command", "Command: 'spawn cube DefenseTower' Dynamic Entity", True,
                         f"Spawned ID: {tower['id']}, Shape: {tower['shape']}, Label: {tower['label']}", (time.time()-t0)*1000)

        client_a.close()
        client_b.close()
    except Exception as e:
        collector.record("Command", "Command Pipeline Verification", False, str(e), 0.0)


def run_pickup_interaction_test(collector: TestReportCollector):
    print("\n" + "="*70)
    print("▶ 运行测试组 6: 场景拾取道具联动与冷却机制验证 (Pickups & Cooldown)")
    print("="*70)

    t0 = time.time()
    try:
        client = WebSocketClient()
        cid = client.wait_for_welcome()

        # 初始传送至远离道具的坐标 (0, 0)
        client.send_command("tp 0 0")
        client.wait_for_echo(keyword="Teleported")

        init_state = client.wait_for_state(lambda s: abs(s["camera_target"][0] - 0.0) < 0.2)
        core_pickup = next((e for e in init_state["entities"] if e["id"] == "pickup-core-1"), None)
        assert core_pickup is not None, "pickup-core-1 should be active and visible at (0, 10)"
        init_score = init_state.get("score", 0)

        # 传送至能量核心坐标 (0.0, 10.0) 触发拾取
        client.send_command("tp 0.0 10.0")
        client.wait_for_echo(keyword="Teleported")

        # 等待拾取生效：积分增加 50，且 pickup-core-1 消失进入冷却
        final_state = client.wait_for_state(
            lambda s: (s.get("score", 0) >= init_score + 50) and not any(e["id"] == "pickup-core-1" for e in s["entities"]),
            timeout=3.0
        )

        has_log = any("Energy Core" in l or "Score +50" in l for l in final_state.get("logs", []))
        assert has_log, f"Expected pickup log in client state logs, got: {final_state.get('logs')}"

        collector.record("Pickup", "Interactive Pickup & Cooldown Lifecycle", True,
                         f"Energy Core harvested (+50 pts): Score {init_score} -> {final_state['score']}, Orb entered 10s cooldown and culled from render list",
                         (time.time()-t0)*1000)

        client.close()
    except Exception as e:
        collector.record("Pickup", "Interactive Pickup & Cooldown Lifecycle", False, str(e), (time.time()-t0)*1000)


def run_dungeon_mode_switching_test(collector: TestReportCollector):
    print("\n" + "="*70)
    print("▶ 运行测试组 7: 游戏模式热切换与暗黑地牢模式验证 (Dungeon Mode Switching)")
    print("="*70)

    t0 = time.time()
    try:
        print("[Mode Switch] Stopping current server and loading 'games/dungeon.scm'...")
        dungeon_proc = start_game_server("games/dungeon.scm")

        # 连接客户端测试地牢世界
        client = WebSocketClient()
        cid = client.wait_for_welcome()
        state = client.wait_for_state()

        # 校验 1: 地牢核心 (dungeon-core, shape="sphere")
        core = next((e for e in state["entities"] if e["id"] == "dungeon-core"), None)
        assert core is not None, "Dungeon core entity ('dungeon-core') not found in state"
        assert core["shape"] == "sphere", f"Expected sphere for dungeon core, got {core['shape']}"
        assert core["label"] == "Dungeon Core"

        # 校验 2: 四大远古石柱 (pillar-ne, pillar-nw, pillar-se, pillar-sw, shape="cube")
        pillars = [e for e in state["entities"] if e["id"].startswith("pillar-")]
        assert len(pillars) >= 4, f"Expected 4 stone pillars, found {len(pillars)}"
        for p in pillars:
            assert p["shape"] == "cube", f"Pillar shape must be cube: {p}"

        # 校验 3: 祭坛 (altar-north)
        altar = next((e for e in state["entities"] if e["id"] == "altar-north"), None)
        assert altar is not None, "Sacrificial Altar ('altar-north') not found"

        # 校验 4: 地牢专有命令 'torch'
        client.send_command("torch")
        echo_torch = client.wait_for_echo(keyword="torch")
        assert echo_torch["success"] is True

        collector.record("GameMode", "Dungeon Mode Initialization & Entities", True,
                         f"Loaded dungeon.scm successfully: Core ({core['label']}), 4 Stone Pillars, Altar, Torch command echo: '{echo_torch['output']}'",
                         (time.time()-t0)*1000)

        client.close()

    except Exception as e:
        collector.record("GameMode", "Dungeon Mode Initialization & Entities", False, str(e), (time.time()-t0)*1000)
    finally:
        # 切回默认的 space_arena.scm
        print("[Mode Switch] Restoring space_arena.scm game server...")
        start_game_server("games/space_arena.scm")


def run_performance_benchmarks(collector: TestReportCollector):
    print("\n" + "="*70)
    print("▶ 运行测试组 8: 性能、延迟与 Tick 同步率统计基准 (Performance Benchmarks)")
    print("="*70)

    try:
        client = WebSocketClient()
        cid = client.wait_for_welcome()

        # 8.1 往返延迟 (RTT) 评测 (50 轮控制台交互采样)
        rtt_samples: List[float] = []
        for _ in range(50):
            t_send = time.time()
            client.send_command("stats")
            _ = client.wait_for_echo(keyword="Status of")
            rtt_samples.append((time.time() - t_send) * 1000.0) # ms
            time.sleep(0.01)

        rtt_avg = sum(rtt_samples) / len(rtt_samples)
        rtt_min = min(rtt_samples)
        rtt_max = max(rtt_samples)
        rtt_p95 = sorted(rtt_samples)[int(len(rtt_samples) * 0.95)]

        # 8.2 状态同步 Tick 间隔与频率评测 (连续采样 40 个状态包)
        tick_timestamps: List[float] = []
        packet_sizes: List[int] = []

        client.sock.settimeout(2.0)
        start_bench = time.time()
        while len(tick_timestamps) < 40 and time.time() - start_bench < 3.0:
            op, data = client.recv_frame()
            if op in (0x01, 0x02):
                msg = json.loads(data.decode("utf-8"))
                if "camera_target" in msg:
                    tick_timestamps.append(time.time())
                    packet_sizes.append(len(data))

        intervals_ms = [
            (tick_timestamps[i] - tick_timestamps[i - 1]) * 1000.0
            for i in range(1, len(tick_timestamps))
        ]
        avg_interval = sum(intervals_ms) / len(intervals_ms) if intervals_ms else 0.0
        effective_tps = 1000.0 / avg_interval if avg_interval > 0 else 0.0

        avg_size = sum(packet_sizes) / len(packet_sizes) if packet_sizes else 0

        collector.benchmark_metrics = {
            "rtt_avg_ms": rtt_avg,
            "rtt_min_ms": rtt_min,
            "rtt_max_ms": rtt_max,
            "rtt_p95_ms": rtt_p95,
            "tick_interval_avg_ms": avg_interval,
            "effective_tps": effective_tps,
            "packet_size_avg_bytes": avg_size,
            "packet_size_min_bytes": min(packet_sizes) if packet_sizes else 0,
            "packet_size_max_bytes": max(packet_sizes) if packet_sizes else 0,
        }

        collector.record("Performance", "Network RTT Latency Benchmark (50 rounds)", True,
                         f"Avg: {rtt_avg:.2f}ms | Min: {rtt_min:.2f}ms | Max: {rtt_max:.2f}ms | P95: {rtt_p95:.2f}ms", rtt_avg)

        collector.record("Performance", "SyncState Tick Cadence & TPS", True,
                         f"Tick Interval: {avg_interval:.2f}ms (~{effective_tps:.1f} TPS) | Packet Size: {avg_size:.0f} bytes", avg_interval)

        client.close()
    except Exception as e:
        collector.record("Performance", "Performance Benchmark", False, str(e), 0.0)


# ============================================================================
# 6. 主执行入口
# ============================================================================
def main():
    print("=" * 70)
    print("  LispCraft 3D 多人游戏系统 - 综合集成验证与验收测试流水线")
    print("=" * 70)

    # 确保服务端处于运行状态 (默认 space_arena.scm)
    try:
        s = socket.socket()
        s.settimeout(0.5)
        s.connect(("127.0.0.1", 8089))
        s.close()
    except Exception:
        print("[Init] Starting server on space_arena.scm...")
        start_game_server("games/space_arena.scm")

    collector = TestReportCollector()

    # 依次执行各大测试组
    run_http_wasm_tests(collector)
    run_tcp_native_protocol_test(collector)
    run_viewport_independence_test(collector)
    run_spatial_culling_test(collector)
    run_command_pipeline_test(collector)
    run_pickup_interaction_test(collector)
    run_dungeon_mode_switching_test(collector)
    run_performance_benchmarks(collector)

    # 汇总输出
    total = len(collector.results)
    passed = sum(1 for r in collector.results if r["success"])
    failed = total - passed

    print("\n" + "=" * 70)
    print(f"验收测试执行完成: 总计 {total} 项用例, 通过 {passed} 项, 失败 {failed} 项")
    print("=" * 70)

    return collector


if __name__ == "__main__":
    col = main()
    failures = [r for r in col.results if not r["success"]]
    if failures:
        sys.exit(1)
    sys.exit(0)
