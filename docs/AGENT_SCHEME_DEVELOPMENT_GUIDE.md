# LispCraft 3D 游戏开发指南（面向 Agent）
# Agent-First Scheme Game Development Manual

> **致 AI Agent 开发者**：  
> LispCraft 3D 是一个由纯 Scheme（Steel Scheme）驱动的 3D 联机/单机同构游戏引擎。底层的 Rust/Macroquad 仅作为高性能、无状态的图形与网络宿主。  
> **你完全不需要了解任何 Rust 代码或修改 Rust 工程**，仅通过编写 Scheme 脚本（S-Expression），就能实现完整的 3D 游戏玩法、网络同步、物理判定、自定义命令系统以及 100% 自由度的 3D/2D 渲染管线。

---

## 目录 (Table of Contents)

1. [核心世界模型与架构原理](#1-核心世界模型与架构原理)
2. [游戏工程标准目录规范](#2-游戏工程标准目录规范)
3. [服务端契约 (Server Authoritative Scheme)](#3-服务端契约-server-authoritative-scheme)
4. [客户端契约与 GPU 渲染原子 (Client Scheme & GPU Atoms)](#4-客户端契约与-gpu-渲染原子-client-scheme--gpu-atoms)
5. [双端事件管道 (RemoteEvents RPC)](#5-双端事件管道-remoteevents-rpc)
6. [Steel Scheme 方言核心与 Agent 实用函数库](#6-steel-scheme-方言核心与-agent-实用函数库)
7. [Agent 避坑指南与最佳实践](#7-agent-避坑指南与最佳实践)
8. [实战教学：从零开发《水晶猎人》完整游戏](#8-实战教学从零开发水晶猎人完整游戏)
9. [运行、调试与验证指南](#9-运行调试与验证指南)

---

## 1. 核心世界模型与架构原理

LispCraft 采用经典且经过工业验证的 **Authoritative Server + Client Prediction + Scheme-Driven Rendering**（服务端权威仿真 + 客户端本地预测 + Scheme 驱动渲染）架构：

```
                    ┌────────────────────────────────────────────────────────┐
                    │               Server VM (20 TPS 时钟周期)               │
                    │  (game-tick dt) authoritative simulation               │
                    │  (on-player-move id dx dz yaw)                         │
                    │  (get-client-state-json id) -> 25m 空间裁剪 Snapshot   │
                    └───────────────────────────┬────────────────────────────┘
                                                │ WebSocket (Layer 0)
                                                │ Snapshot (20 Hz) / RemoteEvents
                                                ▼
┌────────────────────────────────────────────────────────────────────────────────────────┐
│                              Client VM (60 FPS+ 视频帧渲染)                            │
│  1. (get-local-player)       -> 获取本地主角坐标、偏航角、血量、积分                    │
│  2. (get-replicated-entities)-> 获取视距内服务端同步的所有实体列表                      │
│  3. (draw-cube! ...)         -> 3D 图元渲染 (空间、场景、其他玩家 Avatar)              │
│  4. (draw-rect! ...)         -> 2D UI 渲染 (HUD、仪表盘、小地图、准星)                 │
│  5. (fire-server! ...)       -> 上报离散交互事件至服务端                                │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

- **服务端权威 (Server Authoritative)**：
  - 运行在 20 TPS（每秒 20 次，`dt = 0.05s`）的时钟循环下。
  - 管理全部游戏对象、生命周期、碰撞拾取、积分结算与 25 米视距空间裁剪。
  - 每次 Tick 针对每位在线玩家单独生成一份 JSON 状态快照下发。
- **客户端呈现 (Client Presentation)**：
  - 运行在 60 FPS+（随显示器刷新率，`dt ≈ 0.016s`）的无死锁协程中。
  - 客户端通过内建的 GPU 绘图原子，100% 决定屏幕上每一个立方体、球体、网格、文字、血条、小地图的外形与排布。
  - 客户端没有写死的任何 3D 逻辑，**Scheme 脚本决定游戏的一切外观与交互**。

---

## 2. 游戏工程标准目录规范

服务端采用**多游戏动态挂载**模式。所有游戏存放在 `games/<game_id>/` 目录下：

```text
games/<game_id>/
├── config.json          # 游戏元数据配置 (必须)
├── server/              # 服务端权威逻辑目录 (必须)
│   └── main.scm         # 服务端入口脚本 (必须)
├── client/              # 客户端渲染与表现目录 (必须)
│   ├── main.scm         # 客户端主入口脚本 (必须)
│   ├── avatar.scm       # 可选子模块：机甲与角色形象渲染
│   └── hud.scm          # 可选子模块：2D UI 与仪表盘
└── shared/              # 跨端共享代码库 (可选)
    └── common.scm       # 双端通用的纯函数/常量/数学库
```

### 2.1 `config.json` 规范
每个游戏必须在根目录下包含一个 `config.json`：
```json
{
  "name": "Space Arena",
  "version": "1.0.0",
  "view_distance": 25.0,
  "description": "Multiplayer cyberpunk space arena combat simulation"
}
```
- `name`：游戏名称（显示在服务控制台）。
- `view_distance`：空间裁剪视距常量（默认 25.0 米）。
- 启动服务后，浏览器直接打开 `http://<ip>:8082/<game_id>/<player_token>` 即可自动加载该游戏。

### 2.2 模块化与自动虚拟文件系统分发 (Virtual VFS)
- 客户端 `client/` 下的所有 `.scm` 文件在建立连接时会被服务端打包为只读虚拟文件系统自动下发。
- 模块载入规范：`client/main.scm` 会优先执行，若拆分了多个文件，使用 `(load "avatar.scm")` 即可无缝加载。

---

## 3. 服务端契约 (Server Authoritative Scheme)

在 `games/<game_id>/server/main.scm` 中，你必须实现以下指定的顶层函数。底层宿主引擎会在特定时序回调它们。

### 3.1 核心生命周期回调

#### `(init-game)`
- **时机**：服务端加载游戏脚本实例时仅执行一次。
- **入参**：无。
- **用途**：初始化全局实体表、拾取物、地形网格数据。

#### `(on-player-join id)`
- **时机**：新玩家接入 WebSocket 并通过握手认证。
- **入参**：`id` (String，例如 `"guest_412"`)。
- **用途**：创建玩家数据结构（坐标、血量、属性、日志）并记录到全局玩家哈希表。

#### `(on-player-leave id)`
- **时机**：玩家网络断开或离开游戏。
- **入参**：`id` (String)。
- **用途**：从全局玩家表中移除对应数据，清理其产生的临时实体。

#### `(on-player-move id dx dz yaw)`
- **时机**：客户端玩家产生位移输入时（每 Tick 最多触发一次）。
- **入参**：
  - `id` (String)：玩家标识。
  - `dx` (Number)：X 轴归一化位移向量分量（-1.0 ~ 1.0）。
  - `dz` (Number)：Z 轴归一化位移向量分量（-1.0 ~ 1.0）。
  - `yaw` (Number)：摄像机当前偏航弧度（Radian）。
- **位移计算基准规范**：
  - 引擎基准速度为 **5.0 米/秒**。
  - 在 20 TPS（`dt = 0.05s`）下，单 Tick 基础步长为 `0.25 米`。
  ```scheme
  (define (on-player-move id dx dz yaw)
    (let ((p (hash-try-get *players* id)))
      (if p
          (let* ((speed 0.25)
                 (new-x (+ (hash-get p "x") (* dx speed)))
                 (new-z (+ (hash-get p "z") (* dz speed)))
                 (updated (hash-insert (hash-insert (hash-insert p "x" new-x) "z" new-z) "yaw" yaw)))
            (set! *players* (hash-insert *players* id updated))
            "moved")
          "not-found")))
  ```

#### `(game-tick dt)` (别名 `tick-game`)
- **时机**：由服务端主循环以 20 TPS 高频驱动（`dt ≈ 0.05`）。
- **入参**：`dt` (Float，单位秒，通常为 0.05)。
- **用途**：推进物理世界、动画累积时钟、生成怪/金币刷新计时器、拾取物冷却倒计时等。

#### `(on-player-command id cmd)`
- **时机**：玩家在客户端按下回车输入控制台指令（如 `tp 0 0`, `heal`, `spawn cube Box`）。
- **入参**：
  - `id` (String)：操作玩家。
  - `cmd` (String)：原始文本命令字符串。
- **返回值**：String（输出回显内容，将自动转发给客户端控制台面板）。

---

### 3.2 状态快照序列化契约 `(get-client-state-json id)`

这是服务端最核心的函数！底层宿主每 Tick 针对玩家 `id` 调用此函数，获取下发给客户端的 JSON 字符串。

```scheme
(define (get-client-state-json id) -> String)
```

#### JSON 数据格式规范 (严格匹配)
必须输出符合以下 JSON Schema 的纯文本：

```json
{
  "client_id": "guest_412",
  "camera_target": [0.0, 0.5, 0.0],
  "camera_yaw": 0.0,
  "hp": 100.0,
  "max_hp": 100.0,
  "score": 0,
  "entities": [
    {
      "id": "beacon-core",
      "shape": "cube",
      "pos": [0.0, 1.2, 0.0],
      "size": [1.0, 1.0, 1.0],
      "color": [0.2, 0.8, 1.0, 1.0],
      "label": "Beacon Core"
    }
  ],
  "logs": [
    "Welcome to the Arena!",
    "System: Ready."
  ]
}
```

| 字段 | 类型 | 说明 |
| :--- | :--- | :--- |
| `client_id` | String | 当前玩家的唯一标识字符串。 |
| `camera_target` | `[x, y, z]` | 当前玩家在世界中的真实坐标，客户端摄像机和主角 Avatar 将据此同步。 |
| `camera_yaw` | Float | 摄像机偏航角。 |
| `hp` / `max_hp` | Float | 玩家当前血量与生命值上限。 |
| `score` | Integer | 玩家当前计分。 |
| `entities` | Array | 25 米视距内的所有实体列表（场景道具、障碍物、拾取物、**其他在线玩家**）。 |
| `entities[i].id` | String | 实体全局唯一 ID。 |
| `entities[i].shape`| String | 图元基础类型：`"cube"`、`"sphere"` 或任意自定义字符串。 |
| `entities[i].pos` | `[x, y, z]` | 实体中心点 3D 坐标。 |
| `entities[i].size`| `[w, h, d]` | 实体宽高深三维尺寸。 |
| `entities[i].color`| `[r, g, b, a]` | 归一化颜色分量（0.0 ~ 1.0）。 |
| `entities[i].label`| String | 浮动头顶 3D 文字标签（为空时不显示）。 |
| `logs` | Array<String> | 个人终端消息与系统广播队列。 |

> [!TIP]
> **多玩家同服互视的秘诀**：在服务端遍历玩家列表时，把 `(not (string=? pid id))` 的其他玩家包装为一个普通实体（`id: "player-<pid>"`, `shape: "cube"`, `label: pid`），注入到当前玩家的 `entities` 数组中，双端就自动完成了多人同服的相互渲染！

---

## 4. 客户端契约与 GPU 渲染原子 (Client Scheme & GPU Atoms)

在 `games/<game_id>/client/main.scm` 中，你控制客户端的所有视觉呈现。

### 4.1 客户端生命周期

#### `(client-init)`
- **时机**：客户端启动或重新热加载（Hot-Reload）时调用一次。
- **用途**：重置客户端视觉特效时钟、局部动画变量。

#### `(client-render dt)`
- **时机**：宿主**每一帧渲染**时调用（60 FPS+，`dt ≈ 0.016s`）。
- **用途**：调用 GPU 图元绘制原子，先画 3D 背景与实体，再画 2D HUD 界面。

---

### 4.2 本地状态查询原语 (State Queries)

| Scheme 表达式 | 返回值类型 | 说明 |
| :--- | :--- | :--- |
| `(get-local-player)` | AList (关联列表) | 查询自身状态。可用 `(assoc "pos" p)` 获取 `'(x y z)`，`"yaw"` 偏航角，`"hp"` 血量，`"score"` 积分。未连接时返回 `'()`。 |
| `(get-replicated-entities)` | List of AList | 查询当前视野内的全部服务端实体列表。每个实体包含 `"id"`, `"shape"`, `"pos"`, `"size"`, `"color"`, `"label"`。 |
| `(screen-width)` | Float | 获取当前屏幕或浏览器画布的像素宽度（如 `1920.0`）。 |
| `(screen-height)` | Float | 获取当前屏幕或浏览器画布的像素高度（如 `1080.0`）。 |
| `(world-to-screen '(x y z))` | `'(sx sy)` 或 `#f` | 将 3D 世界坐标投影至 2D 屏幕坐标。当点在视锥体背面时返回 `#f`。用于绘制屏幕血条跟随。 |
| `(get-camera)` | `(list target yaw pitch dist)` | 读取当前第三人称环绕摄像机参数。 |
| `(set-camera! target yaw pitch dist)`| Void | 动态重置摄像机观察中心与距离。 |

---

### 4.3 3D GPU 图元绘制原子 (3D Atoms)

所有 3D 绘制函数坐标系为：**X 正右，Y 正上，Z 正后**。必须在 `client-render` 作用域内调用。

#### 1. 立方体与线框
```scheme
;; 填充立方体
(draw-cube! (list x y z) (list width height depth) (list r g b a))

;; 边缘线框立方体 (适合科幻描边效果)
(draw-cube-wires! (list x y z) (list width height depth) (list r g b a))
```

#### 2. 球体与线框
```scheme
;; 填充球体
(draw-sphere! (list x y z) radius (list r g b a))

;; 经纬线球体 (适合护盾、能量球特效)
(draw-sphere-wires! (list x y z) radius (list r g b a))
```

#### 3. 圆柱体 / 棱柱体 / 锥体
```scheme
;; 圆柱/圆锥体: r-top (顶部半径), r-bot (底部半径), height (高度), sides (面数，如 6 为六棱柱，16 为光滑圆柱)
(draw-cylinder! (list x y z) r-top r-bot height sides (list r g b a))
```

#### 4. 平面网格与 3D 线段
```scheme
;; 水平地平面: (draw-plane! 中心坐标 (宽 长) 颜色)
(draw-plane! '(0.0 0.0 0.0) '(100.0 100.0) '(0.05 0.08 0.12 1.0))

;; 空间 3D 线段 (光束、激光、连接线)
(draw-line-3d! (list x1 y1 z1) (list x2 y2 z2) (list r g b a))
```

#### 5. 3D 浮动告示牌文字 (Billboard Text)
```scheme
;; 永远面向摄像机的 3D 空间文字 (适合玩家头顶名字标牌、飘字伤害数值)
(draw-billboard-text! "Boss: Dragon [Lv.99]" (list x (+ y 2.0) z) 16.0 '(1.0 0.9 0.2 1.0))
```

---

### 4.4 2D HUD 与 UI 绘制原子 (2D Screen Atoms)

2D 屏幕绘制坐标系为：**左上角原点 (0, 0)，X 向右延伸，Y 向下延伸**。

```scheme
;; 1. 纯色填充矩形 (HUD 仪表板底色)
(draw-rect! x y width height (list r g b a))

;; 2. 矩形边框
(draw-rect-lines! x y width height thickness (list r g b a))

;; 3. 填充圆形与圆形线框 (技能冷却槽、雷达底盘)
(draw-circle! center-x center-y radius (list r g b a))
(draw-circle-lines! center-x center-y radius thickness (list r g b a))

;; 4. 2D 屏幕线段 (准星准心、连线)
(draw-line-2d! x1 y1 x2 y2 thickness (list r g b a))

;; 5. 屏幕文本渲染
(draw-text-screen! "HP: 100 / 100" 20.0 40.0 18.0 '(1.0 1.0 1.0 1.0))

;; 6. 文本宽度测量 (用于 UI 居中对齐)
(let ((w (measure-text-width "HELLO" 18.0)))
  (draw-text-screen! "HELLO" (- (/ (screen-width) 2.0) (/ w 2.0)) 100.0 18.0 '(1.0 1.0 1.0 1.0)))
```

---

## 5. 双端事件管道 (RemoteEvents RPC)

除了每秒 20 次的快照同步，LispCraft 还提供了类 Roblox 机制的**双向任意 JSON 离散事件通道**。

### 5.1 客户端 -> 服务端
- **客户端发送**：
  ```scheme
  ;; 在客户端主逻辑触发
  (fire-server! "cast-spell" "{\"spell\":\"fireball\",\"power\":50}")
  ```
- **服务端响应**：
  在 `server/main.scm` 中定义 `(on-remote-event player event payload)`：
  ```scheme
  (define (on-remote-event player event payload)
    (cond
      ((string=? event "cast-spell")
       (displayln (string-append "Player " player " casted spell with payload: " payload)))
      (else #f)))
  ```

### 5.2 服务端 -> 客户端
- **服务端单播 (Unicast)**：
  ```scheme
  (fire-client! "guest_1" "level-up" "{\"new_level\": 5}")
  ```
- **服务端广播 (Broadcast)**：
  ```scheme
  (broadcast-client! "world-boss-spawned" "{\"name\": \"Ancient Dragon\"}")
  ```
- **客户端监听与注册**：
  在 `client/main.scm` 中通过 `(on-server-event event-name callback)` 注册处理器：
  ```scheme
  (on-server-event "level-up"
    (lambda (payload)
      (displayln (string-append "Got level-up event: " payload))))
  ```

---

## 6. Steel Scheme 方言核心与 Agent 实用函数库

LispCraft 宿主内嵌的是 **Steel Scheme**。以下整理了 Agent 开发游戏最常用的函数与操作习惯。

### 6.1 持久化哈希表 (Persistent Hash Maps)
游戏状态（如玩家表、背包、配置）推荐存储在持久化哈希表中：

```scheme
;; 1. 创建哈希表
(define my-map (hash "x" 10.0 "y" 0.0 "name" "hero"))

;; 2. 读取键值 (不存在报错)
(hash-get my-map "x") ;; => 10.0

;; 3. 安全读取 (不存在返回 #f)
(hash-try-get my-map "z") ;; => #f

;; 4. 插入/更新键值 (返回新的哈希表，原始不可变)
(define next-map (hash-insert my-map "x" 15.0))

;; 5. 获取所有 Key 的列表
(define keys (hash-keys->list my-map)) ;; => '("x" "y" "name")
```

### 6.2 关联列表 (Association Lists / AList)
客户端 `(get-local-player)` 和 `(get-replicated-entities)` 返回的是标准 AList。

```scheme
(define player (get-local-player))

;; 提取字段值 (支持字符串键与符号键)
(define pos (cdr (assoc "pos" player)))   ;; => '(x y z)
(define yaw (cdr (assoc "yaw" player)))   ;; => 0.0
(define hp (cdr (assoc "hp" player)))     ;; => 100.0

;; 提取 3D 坐标分量
(define px (list-ref pos 0))
(define py (list-ref pos 1))
(define pz (list-ref pos 2))
```

### 6.3 实用数学与几何工具库 (可直接粘贴使用)

```scheme
;; 两点欧几里得距离平方 (避免高频 sqrt 开销)
(define (dist-sq x1 z1 x2 z2)
  (let ((dx (- x1 x2))
        (dz (- z1 z2)))
    (+ (* dx dx) (* dz dz))))

;; 两点三维距离平方
(define (dist3d-sq p1 p2)
  (let ((dx (- (list-ref p1 0) (list-ref p2 0)))
        (dy (- (list-ref p1 1) (list-ref p2 1)))
        (dz (- (list-ref p1 2) (list-ref p2 2))))
    (+ (* dx dx) (* dy dy) (* dz dz))))

;; 线性插值 Lerp: a + (b - a) * t
(define (lerp a b t)
  (+ a (* (- b a) t)))

;; 数值截断 Clamp
(define (clamp val min-val max-val)
  (max min-val (min max-val val)))

;; 字符串按空白切分
(define (split-whitespace str)
  (filter (lambda (s) (> (string-length s) 0))
          (string-split str " ")))
```

---

## 7. Agent 避坑指南与最佳实践

作为 AI Agent，在生成 Scheme 脚本时请严格遵循以下铁律：

### 规则 1：坐标与尺寸必须使用浮点数
Steel Scheme 中如果把整数与浮点数进行隐式混用，部分算子可能会保留整数截断。
- ❌ 错误：`(draw-cube! '(0 1 0) '(1 1 1) '(1 0 0 1))`
- ✅ 正确：`(draw-cube! '(0.0 1.0 0.0) '(1.0 1.0 1.0) '(1.0 0.0 0.0 1.0))`

### 规则 2：构造 JSON 字符串时务必转义
在 `(get-client-state-json id)` 中手动拼接 JSON 时，玩家名字或聊天内容可能包含双引号。必须用转义或过滤：
```scheme
(define (safe-json-string s)
  (string-replace (string-replace s "\\" "\\\\") "\"" "\\\""))
```

### 规则 3：持久化哈希表的 `set!` 模式
Steel 的哈希表是函数式持久化数据结构，`(hash-insert h k v)` **不会就地修改原表**，必须把返回值重新 `set!` 赋回全局变量：
```scheme
;; ❌ 错误：原 *players* 没有任何改变！
(hash-insert *players* id updated-player)

;; ✅ 正确：
(set! *players* (hash-insert *players* id updated-player))
```

### 规则 4：客户端渲染循环不能执行死循环或阻塞操作
`client-render` 是在 UI 渲染线程每帧调用的。
- 绝不要在里面写无休止递归、大循环或耗时过长的运算；
- 避免在每帧分配超大列表，优先重用全局或局部轻量变量。

---

## 8. 实战教学：从零开发《水晶猎人》完整游戏

下面我们通过一个极简但功能齐备的游戏范例 **`crystal_hunter`**，演示如何仅靠 3 个文件完成一个包含 **3D 旋转金币、金币收集判定、计分 HUD、空间声光效果与 `tp`/`stats` 自定义指令** 的完整游戏。

### 步骤 1：创建 `games/crystal_hunter/config.json`
```json
{
  "name": "Crystal Hunter",
  "version": "1.0.0",
  "view_distance": 25.0,
  "description": "Collect sparkling energon crystals in a futuristic grid world."
}
```

### 步骤 2：编写服务端逻辑 `games/crystal_hunter/server/main.scm`
```scheme
;; ========================================================
;; games/crystal_hunter/server/main.scm
;; ========================================================

(define *players* (hash))       ;; 玩家数据表: id -> {x, z, yaw, score, logs}
(define *crystals* '())         ;; 场景水晶列表: list of {id, x, z, active, timer}
(define *game-time* 0.0)

;; 两点距离平方
(define (dist-sq x1 z1 x2 z2)
  (let ((dx (- x1 x2))
        (dz (- z1 z2)))
    (+ (* dx dx) (* dz dz))))

;; 1. 游戏初始化
(define (init-game)
  (displayln "[Server] Initializing Crystal Hunter...")
  (set! *players* (hash))
  (set! *game-time* 0.0)
  ;; 在四周随机生成 6 颗发光能量水晶
  (set! *crystals*
    (list
      (hash "id" "cry-1" "x" 5.0  "z" 5.0  "active" #t "timer" 0.0)
      (hash "id" "cry-2" "x" -8.0 "z" 6.0  "active" #t "timer" 0.0)
      (hash "id" "cry-3" "x" 10.0 "z" -7.0 "active" #t "timer" 0.0)
      (hash "id" "cry-4" "x" -6.0 "z" -9.0 "active" #t "timer" 0.0)
      (hash "id" "cry-5" "x" 0.0  "z" 12.0 "active" #t "timer" 0.0)
      (hash "id" "cry-6" "x" 0.0  "z" -12.0 "active" #t "timer" 0.0))))

;; 2. 玩家生命周期
(define (on-player-join id)
  (displayln (string-append "[Server] Pilot joined: " id))
  (let ((p (hash "x" 0.0
                 "z" 0.0
                 "yaw" 0.0
                 "hp" 100.0
                 "max-hp" 100.0
                 "score" 0
                 "logs" (list "System: Welcome to Crystal Hunter! Move with WASD to collect crystals."))))
    (set! *players* (hash-insert *players* id p))))

(define (on-player-leave id)
  (displayln (string-append "[Server] Pilot left: " id))
  (set! *players* (hash-insert *players* id #f)))

;; 3. 拾取检测逻辑
(define (check-pickups id)
  (let ((p (hash-try-get *players* id)))
    (if (and p (hash? p))
        (let ((px (hash-get p "x"))
              (pz (hash-get p "z")))
          (set! *crystals*
            (map (lambda (cry)
                   (if (and (hash-get cry "active")
                            (< (dist-sq px pz (hash-get cry "x") (hash-get cry "z")) 2.25))
                       (begin
                         ;; 拾取成功：加 10 分并记录日志
                         (let* ((old-score (hash-get p "score"))
                                (new-score (+ old-score 10))
                                (new-logs (cons (string-append "Harvested crystal! Score: " (number->string new-score))
                                                (hash-get p "logs")))
                                (updated-p (hash-insert (hash-insert p "score" new-score) "logs" new-logs)))
                           (set! *players* (hash-insert *players* id updated-p)))
                         ;; 水晶进入 8 秒重生倒计时
                         (hash-insert (hash-insert cry "active" #f) "timer" 8.0))
                       cry))
                 *crystals*)))
        #f)))

;; 4. 玩家位移
(define (on-player-move id dx dz yaw)
  (let ((p (hash-try-get *players* id)))
    (if (and p (hash? p))
        (let* ((speed 0.25)
               (new-x (+ (hash-get p "x") (* dx speed)))
               (new-z (+ (hash-get p "z") (* dz speed)))
               (updated (hash-insert (hash-insert (hash-insert p "x" new-x) "z" new-z) "yaw" yaw)))
          (set! *players* (hash-insert *players* id updated))
          (check-pickups id)
          "moved")
        "not-found")))

;; 5. 20 TPS 世界仿真 (水晶重生倒计时)
(define (game-tick dt)
  (set! *game-time* (+ *game-time* dt))
  (set! *crystals*
    (map (lambda (cry)
           (if (not (hash-get cry "active"))
               (let ((rem (- (hash-get cry "timer") dt)))
                 (if (<= rem 0.0)
                     (hash-insert (hash-insert cry "active" #t) "timer" 0.0)
                     (hash-insert cry "timer" rem)))
               cry))
         *crystals*)))

;; 6. 控制台控制指令 (tp / stats)
(define (on-player-command id cmd)
  (let ((p (hash-try-get *players* id)))
    (if (and p (hash? p))
        (cond
          ((string=? cmd "tp 0 0")
           (set! *players* (hash-insert *players* id (hash-insert (hash-insert p "x" 0.0) "z" 0.0)))
           "Teleported to origin.")
          ((string=? cmd "stats")
           (string-append "Pilot " id " - Score: " (number->string (hash-get p "score"))))
          (else (string-append "Echo: " cmd)))
        "Player not found")))

;; 7. 生成个性化 25m 快照 JSON
(define (get-client-state-json id)
  (let ((p (hash-try-get *players* id)))
    (if (and p (hash? p))
        (let* ((px (hash-get p "x"))
               (pz (hash-get p "z"))
               ;; 筛选 25m 视距内激活的水晶
               (visible-crystals
                 (filter (lambda (c)
                           (and (hash-get c "active")
                                (<= (dist-sq px pz (hash-get c "x") (hash-get c "z")) 625.0)))
                         *crystals*))
               (crystals-json
                 (string-join
                   (map (lambda (c)
                          (string-append
                            "{\"id\":\"" (hash-get c "id") "\","
                            "\"shape\":\"sphere\","
                            "\"pos\":[" (number->string (hash-get c "x")) ",1.0," (number->string (hash-get c "z")) "],"
                            "\"size\":[0.8,0.8,0.8],"
                            "\"color\":[0.2,0.9,1.0,0.85],"
                            "\"label\":\"Energy Crystal\"}"))
                        visible-crystals)
                   ","))
               ;; 日志序列化
               (logs (hash-get p "logs"))
               (logs-json
                 (string-join
                   (map (lambda (m) (string-append "\"" m "\"")) (if (null? logs) '("Sector Normal") logs))
                   ",")))
          (string-append
            "{"
            "\"client_id\":\"" id "\","
            "\"camera_target\":[" (number->string px) ",0.5," (number->string pz) "],"
            "\"camera_yaw\":" (number->string (hash-get p "yaw")) ","
            "\"hp\":100.0,\"max_hp\":100.0,"
            "\"score\":" (number->string (hash-get p "score")) ","
            "\"entities\":[" crystals-json "],"
            "\"logs\":[" logs-json "]"
            "}"))
        "{}")))
```

### 步骤 3：编写客户端渲染 `games/crystal_hunter/client/main.scm`
```scheme
;; ========================================================
;; games/crystal_hunter/client/main.scm
;; ========================================================

(define *client-time* 0.0)

(define (client-init)
  (displayln "[Client] Crystal Hunter Renderer Initialized.")
  (set! *client-time* 0.0))

;; 绘制单个远端实体 (水晶球体带有呼吸灯特效)
(define (draw-entity ent)
  (let* ((pos (cdr (assoc "pos" ent)))
         (shape (cdr (assoc "shape" ent)))
         (label (cdr (assoc "label" ent)))
         (px (list-ref pos 0))
         (py (list-ref pos 1))
         (pz (list-ref pos 2)))
    ;; 上下浮动呼吸动画
    (let ((bob-y (+ py (* 0.2 (sin (* *client-time* 3.0))))))
      (draw-sphere! (list px bob-y pz) 0.45 '(0.1 0.85 1.0 0.8))
      (draw-sphere-wires! (list px bob-y pz) 0.52 '(0.8 0.95 1.0 0.6))
      (if (and label (> (string-length label) 0))
          (draw-billboard-text! label (list px (+ bob-y 0.9) pz) 13.0 '(0.4 1.0 0.9 0.95))
          #f))))

;; 主渲染循环 (由 Rust 宿主 60 FPS 每帧调用)
(define (client-render dt)
  (set! *client-time* (+ *client-time* dt))

  ;; 1. 3D 科幻地平面与网格
  (draw-plane! '(0.0 0.0 0.0) '(80.0 80.0) '(0.03 0.04 0.07 1.0))
  (let loop ((x -30.0))
    (if (<= x 30.0)
        (begin
          (draw-line-3d! (list x 0.01 -30.0) (list x 0.01 30.0) '(0.1 0.25 0.35 0.3))
          (draw-line-3d! (list -30.0 0.01 x) (list 30.0 0.01 x) '(0.1 0.25 0.35 0.3))
          (loop (+ x 3.0)))
        #f))

  ;; 2. 渲染所有同步实体 (水晶)
  (let ((entities (get-replicated-entities)))
    (if (list? entities)
        (for-each draw-entity entities)
        #f))

  ;; 3. 渲染自身主角机甲
  (let ((me (get-local-player)))
    (if (and (list? me) (assoc "pos" me))
        (let* ((pos (cdr (assoc "pos" me)))
               (px (list-ref pos 0))
               (py (list-ref pos 1))
               (pz (list-ref pos 2))
               (score (cdr (assoc "score" me))))
          ;; 主角底盘与机体
          (draw-cube! (list px (+ py 0.5) pz) '(0.8 0.8 0.8) '(0.2 0.8 0.4 1.0))
          (draw-cube-wires! (list px (+ py 0.5) pz) '(0.82 0.82 0.82) '(1.0 1.0 1.0 0.8))
          (draw-cylinder! (list px (+ py 0.05) pz) 0.7 0.7 0.05 12 '(0.2 0.8 0.4 0.4))

          ;; 4. 2D HUD 积分榜
          (let ((sw (screen-width))
                (sh (screen-height)))
            ;; 顶栏面板
            (draw-rect! 20.0 20.0 240.0 45.0 '(0.05 0.08 0.12 0.85))
            (draw-rect-lines! 20.0 20.0 240.0 45.0 1.5 '(0.2 0.8 0.5 0.9))
            (draw-text-screen! "CRYSTAL HUNTER" 32.0 42.0 15.0 '(0.2 0.9 0.5 1.0))
            (draw-text-screen! (string-append "SCORE: " (number->string score)) 32.0 58.0 13.0 '(1.0 0.85 0.2 1.0))))
        #f)))
```

---

## 9. 运行、调试与验证指南

无需对底层做任何编译或改动，你写好的游戏目录即可直接通过宿主二进制运行并提供测试服务：

### 9.1 启动服务并访问
```bash
# 启动服务端 (自动挂载 crystal_hunter)
./dist/lispcraft-linux-arm64/server games/crystal_hunter

# 浏览器访问
http://localhost:8082/crystal_hunter/agent_pilot_1
```

### 9.2 本地离线热重载调试 (Zero-Server Standalone Mode)
在单机调试客户端视觉效果时，**甚至无需开启服务端**：
```bash
./dist/lispcraft-linux-arm64/client games/crystal_hunter/client
```
此时宿主会实时监控 `games/crystal_hunter/client/` 目录。**当你在该目录下修改或新建任何 `.scm` 代码时，无需重启客户端，下一帧自动热重载生效**，实现秒级高频迭代！

### 9.3 验证检查清单 (Verification Checklist for Agents)
- [ ] 运行 `./build.sh --test` 确认全量集成测试无回归。
- [ ] 确保 `(get-client-state-json id)` 在任何极端情况（如玩家数据未准备好）均返回合法 JSON（至少为 `"{}"`）。
- [ ] 确保在 `client-render` 绘制 3D 元素前已做好 `(list? entities)` 空指针保护。
- [ ] 检查所有 3D 和 2D 尺寸参数无负数，颜色分量在 `0.0 ~ 1.0` 闭区间内。

---
*文档版本: v1.0.0 | 维护体系: LispCraft 3D Core Team*
