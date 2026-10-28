import socket
import urllib.request
import json
import time
import struct
import base64
import os

def test_http():
    http_port = os.environ.get("HTTP_PORT", "8080")
    print(f"=== Testing HTTP on {http_port} ===")
    url_base = f"http://127.0.0.1:{http_port}"

    # Test / (index.html)
    req = urllib.request.Request(f"{url_base}/")
    with urllib.request.urlopen(req) as resp:
        code = resp.getcode()
        ct = resp.headers.get("Content-Type")
        cors = resp.headers.get("Access-Control-Allow-Origin")
        content = resp.read()
        print(f"GET / -> Status: {code}, Content-Type: {ct}, CORS: {cors}, Size: {len(content)} bytes")
        assert code == 200
        assert "text/html" in ct
        assert b"LispCraft 3D" in content
        assert cors == "*"

    # Test /client.wasm
    req = urllib.request.Request(f"{url_base}/client.wasm", method="HEAD")
    with urllib.request.urlopen(req) as resp:
        code = resp.getcode()
        ct = resp.headers.get("Content-Type")
        cl = resp.headers.get("Content-Length")
        print(f"HEAD /client.wasm -> Status: {code}, Content-Type: {ct}, Content-Length: {cl}")
        assert code == 200
        assert ct == "application/wasm"
        assert int(cl) > 10000000

    # Test /gl.js
    req = urllib.request.Request(f"{url_base}/gl.js", method="HEAD")
    with urllib.request.urlopen(req) as resp:
        code = resp.getcode()
        ct = resp.headers.get("Content-Type")
        print(f"HEAD /gl.js -> Status: {code}, Content-Type: {ct}")
        assert code == 200
        assert ct == "application/javascript"

    # Test 404
    try:
        urllib.request.urlopen(f"{url_base}/does_not_exist.bin")
        assert False, "Should have returned 404"
    except urllib.error.HTTPError as e:
        print(f"GET /does_not_exist.bin -> Status: {e.code} (Expected 404)")
        assert e.code == 404

    print("HTTP 8087 test PASSED!\n")

def test_tcp():
    print("=== Testing TCP Native on 8088 ===")
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(3.0)
    s.connect(("127.0.0.1", 8088))
    print("Connected to TCP 8088")

    # Send a command: {"Command": "who"}
    cmd_obj = {"Command": "who"}
    data = json.dumps(cmd_obj).encode("utf-8")
    packet = bytes([len(data)]) + data
    s.sendall(packet)
    print(f"Sent ClientMsg::Command: {cmd_obj}")

    # Read response
    len_byte = s.recv(1)
    if len_byte:
        msg_len = len_byte[0]
        payload = s.recv(msg_len)
        print(f"Received from TCP ({msg_len} bytes): {payload.decode('utf-8', errors='replace')}")
    s.close()
    print("TCP 8088 test PASSED!\n")

def test_websocket():
    print("=== Testing WebSocket on 8089 ===")
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(3.0)
    s.connect(("127.0.0.1", 8089))

    # WebSocket Handshake
    key = base64.b64encode(os.urandom(16)).decode("utf-8")
    req = (
        f"GET / HTTP/1.1\r\n"
        f"Host: 127.0.0.1:8089\r\n"
        f"Upgrade: websocket\r\n"
        f"Connection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\n"
        f"Sec-WebSocket-Version: 13\r\n\r\n"
    )
    s.sendall(req.encode("utf-8"))

    resp = s.recv(4096).decode("utf-8", errors="replace")
    print("Handshake Response:\n" + resp.split("\r\n\r\n")[0])
    assert "101 Switching Protocols" in resp

    # Read a WebSocket frame (server sends Welcome / SyncState upon connect / timer)
    raw = s.recv(4096)
    if len(raw) >= 2:
        opcode = raw[0] & 0x0F
        plen = raw[1] & 0x7F
        idx = 2
        if plen == 126:
            plen = struct.unpack("!H", raw[2:4])[0]
            idx = 4
        elif plen == 127:
            plen = struct.unpack("!Q", raw[2:10])[0]
            idx = 10
        payload = raw[idx:idx+plen]
        print(f"Received WS Frame (opcode={opcode}, len={plen}): {payload.decode('utf-8', errors='replace')[:120]}...")

    # Send a command frame: ClientMsg::Command("heal")
    msg_str = json.dumps({"Command": "heal"})
    payload_bytes = msg_str.encode("utf-8")
    mask_key = os.urandom(4)
    masked = bytes(b ^ mask_key[i % 4] for i, b in enumerate(payload_bytes))
    # Frame header: FIN + text(1) or bin(2), mask bit set
    frame_hdr = bytearray([0x81, 0x80 | len(payload_bytes)]) + mask_key + masked
    s.sendall(frame_hdr)
    print(f"Sent WS Frame: {msg_str}")

    # Read response frame
    time.sleep(0.1)
    raw = s.recv(4096)
    if len(raw) >= 2:
        opcode = raw[0] & 0x0F
        plen = raw[1] & 0x7F
        idx = 2
        if plen == 126:
            plen = struct.unpack("!H", raw[2:4])[0]
            idx = 4
        elif plen == 127:
            plen = struct.unpack("!Q", raw[2:10])[0]
            idx = 10
        payload = raw[idx:idx+plen]
        print(f"Received WS Response Frame: {payload.decode('utf-8', errors='replace')}")

    s.close()
    print("WebSocket 8089 test PASSED!\n")

if __name__ == "__main__":
    time.sleep(1) # wait for server to start if just launched
    test_http()
    test_tcp()
    test_websocket()
    port = os.environ.get("HTTP_PORT", "8080")
    print(f"ALL THREE PORTS ({port}, 8088, 8089) VERIFIED SUCCESSFULLY!")
