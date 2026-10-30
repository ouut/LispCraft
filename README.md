# LispCraft 🪐

> **基于 Rust + Steel (Scheme) + Macroquad 3D + 通用底座 (game_net / game_server / game_script v0.2.0) 的极简多人 3D 游戏引擎**

LispCraft 是一个服务端完全主导、客户端纯数据驱动的 3D 多人游戏架构。系统将 **Steel Scheme** 脚本虚拟机嵌入至 Rust 服务端作为唯一的游戏世界逻辑大脑，采用通用游戏基础设施 **Layer 0**（`game_net`, `game_server`, `game_script`）作为通信与安全底座，客户端基于 **Macroquad 3D** 编译为 **WebAssembly** 在浏览器或桌面独立运行，通过 WebSocket 实现跨平台低延迟全双工通信。

---

## 🌟 核心设计理念

1. **客户端与服务端彻底解耦 (Strict Decoupling)**：
   - **客户端是纯粹的 3D 显示器**：客户端代码无任何游戏判定、移动碰撞、血量或规则逻辑，仅依据服务端下发的空间几何图元（立方体、球体、坐标、颜色、标牌）进行渲染。
   - **服务端是唯一的逻辑裁判**：所有玩家移动、道具交互、世界演进均在服务端单一的 Steel Scheme VM 实例中计算。
2. **个性化状态视口，非多人同屏 (Not Shared Screen)**：
   - 每个玩家拥有独立的本地第三人称摄像机与专属状态（生命值、坐标、积分、个人日志）。
   - **25m 空间距离裁剪**：Scheme 依据玩家坐标动态计算视口切片，仅下发视野范围内的实体；进入视野的其他玩家会动态投影为带有对应代号标签的 3D 化身，视距外自动隐去。
3. **单实例多租户 Scheme 运行态 (Scheme-Authoritative World)**：
   - 服务端启动时指定一个 `.scm` 脚本（如 `space_arena.scm` 或 `dungeon.scm`），全局初始化唯一一个 Scheme 虚拟机实例，所有在线玩家共享同一个游戏世界上下文。
4. **混合通信与字符串命令管道 (Dual Input Pipeline)**：
   - 客户端除了捕获键盘 WASD 和鼠标视角移动外，按 `Enter` 键即可唤起赛博命令行输入栏。
   - 用户输入的任意字符串（如 `heal`、`tp 10 10`、`say Hello`、`spawn cube tower`）原样传输至服务端，由 Scheme 解析执行并返回执行结果。
5. **单端口一体化网关 (Single-Port Gateway)**：
   - 基于 `game_server` 构建统一 Axum 网关，单个端口同时托管 WebAssembly 前端静态资产与全双工 WebSocket 端点（`/ws`），注入 `COOP/COEP` 高性能隔离安全头，免去多端口与跨域困扰。
6. **高频锁存流水线与看门狗保护 (Latest-Only Latch & Watchdog)**：
   - 移动操作通过 `game_net::ClientEnvelope` 接入服务端的 **Latest-Only Latch**，每帧只保留最新输入，杜绝操作堆积迟滞。
   - 集成 `game_script` 的 100ms 看门狗安全求值，防止终端死循环脚本导致服务端挂起。
   - 支持心跳保活与 60s 断线优雅重连恢复（Grace Period）。

---

## 🎮 快速开始

### 1. 源码启动 (推荐)

确保已安装 Rust 工具链（1.85+ 及 wasm32 目标）：

```bash
# 克隆仓库
git clone https://github.com/ouut/LispCraft.git
cd LispCraft

# 启动服务端 (加载默认太空竞技场模式，默认端口 8087)
cargo run --release --bin server games/space_arena.scm
```

服务就绪后，在现代浏览器中打开一个或多个标签页访问：
👉 **`http://localhost:8087`**

立即即可体验多玩家各自独立视角的 3D 联机环境！

### 2. 如何指定自定义端口 (Port Configuration)

系统支持通过 **命令行位置参数**、**选项参数** 或 **环境变量** 自由指定网关监听端口（默认 8087）：

```bash
# 方式 A：位置参数
cargo run --release --bin server games/space_arena.scm 8081

# 方式 B：CLI 参数 (-p 或 --port)
cargo run --release --bin server games/space_arena.scm --port 8081

# 方式 C：环境变量 (PORT 或 HTTP_PORT)
PORT=8087 cargo run --release --bin server games/space_arena.scm
```

### 3. 桌面原生客户端启动

服务端保持运行的前提下，可以在本地终端直接启动桌面原生窗口客户端（自动直连服务端默认端口 8087）：

```bash
# 直连默认端口 (8087)
cargo run --release --bin client

# 或指定端口 / 远程地址
cargo run --release --bin client 8081
cargo run --release --bin client -- ws://127.0.0.1:8087/ws
```

桌面客户端与网页端玩家完全互通同服竞技。

### 4. 一键切换游戏世界

服务端可在启动时指定任意 Scheme 脚本，瞬间切换至不同的游戏规则与世界布局：

```bash
# 切换至暗黑地牢模式 (包含祭坛、远古石柱与常驻火把)
cargo run --release --bin server games/dungeon.scm
```

---

## ⌨️ 操作与指令指南

### 快捷键操作
| 操作 | 对应按键 / 输入 |
| :--- | :--- |
| **角色移动** | `W` 前进、`S` 后退、`A` 向左平移、`D` 向右平移（自动基于摄像机偏航角视线对齐） |
| **旋转视角** | 鼠标左键/右键按住拖拽，或键盘 `←` / `→` 键平滑环绕 |
| **俯仰视角** | 鼠标上下拖拽，或键盘 `↑` / `↓` 键平滑俯仰 |
| **镜头缩放** | 鼠标滚轮（Zoom 4.0m ~ 26.0m） |
| **唤起/关闭控制台** | 按 `Enter` 键唤起命令行，按 `Esc` 或空回车退出控制台 |
| **历史命令回溯** | 控制台激活状态下，按键盘 `↑` / `↓` 键在历史指令中翻阅 |

### 内置终端命令表
在控制台中输入以下指令后回车即可生效：

| 指令 | 作用描述 |
| :--- | :--- |
| `help` | 显示当前游戏模式下的全部可用命令与语法格式 |
| `heal` | 瞬间恢复生命值至 100% |
| `tp` | 传送回世界中心主信标 (0, 0) |
| `tp <x> <z>` | 瞬移至指定世界坐标，如 `tp 15 -10` |
| `speed <factor>` | 动态调整自身移动速度倍率（0.1x ~ 5.0x），如 `speed 2.5` |
| `spawn <cube\|sphere> <label>` | 在当前脚下放置带有自定义标签的 3D 建筑，如 `spawn cube MyBase` |
| `say <message>` | 向全服所有在线玩家的通知栏发送广播，如 `say Greetings Galaxy!` |
| `list` | 查看当前全服所有在线玩家 ID 及其浮点坐标 |
| `stats` | 查看当前生命值、移动倍率、积分与三维坐标 |
| `torch` *(地牢模式)* | 在当前位置点燃并永久放置一盏暖光地牢火把 |

---

## 📁 代码工程架构 (Strict 3-Tier Star-Tree DAG)

项目遵循单向树状无环 (DAG) Cargo Workspace 架构规范，依赖公共三底座（`game_net = "0.2.0"`, `game_server = "0.2.0"`, `game_script = "0.2.0"`），各子包正交解耦、禁止反向依赖，支持多 Agent 协同并发开发：

```text
LispCraft/
├── .github/workflows/
│   └── release.yml             # 跨平台 (Win/Mac ARM64/Linux) 编译与自动发布流水线
├── Cargo.toml                  # 统一管理 workspace.dependencies 与 members
├── docs/                       # 架构设计、并发工作流与移植指南
│   ├── REQUIREMENTS_DAG.md     # [架构核心] 3-Tier DAG 需求规格与 Roblox 渲染预测规范
│   ├── MULTI_AGENT_WORKFLOW.md # [协作手册] 多 Agent 并发开发文件所有权隔离与屏障规范
│   ├── CROSS_GAME_PORTING.md   # 三款游戏 Layer 0 通用底座架构规范
│   └── PORTING_TO_LISPCRAFT.md # LispCraft 落地网络与脚本底座实战全解析
├── crates/
│   ├── lc_core/                # [Layer 0 领域契约] 纯协议 DTO、实体描述与 25m 视距常量 (零业务依赖)
│   ├── lc_game/                # [Layer 1 业务插件] LispCraftPlugin、Scheme 规则引擎与 25m 视距裁剪
│   ├── lc_render/              # [Layer 1 渲染系统] OrbitCamera 摄像机、NDC 投影、3D 图元、2D 雷达与 60FPS 平滑插值
│   ├── lc_server/              # [Layer 2 服务端入口] Axum 单端口网关 + 20 TPS 仿真循环 (`bin: server`)
│   └── lc_client/              # [Layer 2 客户端入口] Macroquad 跨平台窗口 + 客户端即时预测器 (`bin: client`)
├── games/                      # Scheme 编写的游戏世界规则包 (*.scm)
│   ├── space_arena.scm         # 太空竞技场模式 (空间裁剪、拾取物、全指令)
│   └── dungeon.scm             # 暗黑地牢探险模式 (迷雾视野、石柱、火把放置)
├── web/                        # WebAssembly 宿主静态资产
│   ├── index.html              # 网页主入口
│   ├── gl.js                   # Macroquad WebGL 驱动桥
│   ├── sapp_jsutils.js         # Miniquad JS 对象交换工具
│   ├── quad-net.js             # quad-net WebSocket 运行时插件
│   └── client.wasm             # 预编译生成的 WebAssembly 二进制
└── tests/
    └── e2e_multiplayer_test.py # 19 项端到端多客户端并发仿真自动化验收测试套件
```

---

## 🚀 跨平台编译与 GitHub Actions 自动发布

本项目配置了完整的 GitHub Actions 持续集成与发布流水线 ([.github/workflows/release.yml](file:///.github/workflows/release.yml))。

### 自动化构建矩阵 (Matrix Targets)
1. **Linux**: `x86_64-unknown-linux-gnu` (Ubuntu 最新运行时)
2. **macOS ARM64**: `aarch64-apple-darwin` (Apple Silicon M 系列芯片)
3. **Windows**: `x86_64-pc-windows-msvc` (Windows x64 原生)
4. **WebAssembly**: `wasm32-unknown-unknown` (浏览器纯前端)

### 触发发布
只需在本地推送版本 Tag，GitHub Actions 将全自动并行编译三端二进制与 WebAssembly 包，并自动打包为包含 `server`、`client`、`games/`、`web/` 的便携安装包发布至 GitHub Releases 页面：

```bash
git tag v0.2.0
git push origin v0.2.0
```

Release 页面将自动生成并附加：
* `lispcraft-linux-x86_64.tar.gz`
* `lispcraft-macos-arm64.tar.gz`
* `lispcraft-windows-x86_64.zip`

---

## 📄 许可证

本项目基于 [MIT](LICENSE) 许可证开源。欢迎提交 Issue 与 Pull Request！
