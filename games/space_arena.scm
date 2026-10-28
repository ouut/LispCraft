;; ========================================================
;; games/space_arena.scm - LispCraft 多人 3D 太空竞技场逻辑
;; ========================================================

;; 玩家哈希表: id -> player-hash
(define *players* (hash))

;; 场景实体列表 (信标、巨石柱等场景核心结构)
(define *entities* '())

;; 玩家通过命令生成的动态实体列表
(define *user-entities* '())

;; 场景中散落的拾取道具能量球列表
(define *pickups* '())

;; 全局游戏时钟累积时间 (秒)
(define *game-time* 0.0)

;; 视口裁剪视野距离 (只有在此距离内的实体与玩家才下发)
(define *view-distance* 25.0)

;; 玩家专属颜色调色板 (RGBA)
(define *player-colors*
  '((0.2 0.85 1.0 1.0)   ;; 电浆青 (Cyber Cyan)
    (1.0 0.35 0.35 1.0)  ;; 猩红 (Crimson Red)
    (0.35 1.0 0.45 1.0)  ;; 荧光绿 (Neon Green)
    (1.0 0.85 0.2 1.0)   ;; 琥珀金 (Amber Gold)
    (0.85 0.4 1.0 1.0)   ;; 灵能紫 (Electric Purple)
    (1.0 0.55 0.15 1.0)  ;; 耀斑橙 (Solar Orange)
    (0.2 1.0 0.85 1.0)   ;; 极光碧 (Mint Aurora)
    (1.0 0.3 0.7 1.0)))  ;; 霓虹粉 (Neon Pink)

;; --------------------------------------------------------
;; 基础辅助函数
;; --------------------------------------------------------

;; 创建实体哈希结构
(define (make-entity id shape x y z sx sy sz r g b a label)
  (hash "id" id "shape" shape "x" x "y" y "z" z
        "sx" sx "sy" sy "sz" sz
        "r" r "g" g "b" b "a" a
        "label" label))

;; 创建可拾取能量球
(define (make-pickup id ptype x base-y z sx sy sz r g b a label)
  (hash "id" id "type" ptype
        "x" x "base-y" base-y "y" base-y "z" z
        "sx" sx "sy" sy "sz" sz
        "r" r "g" g "b" b "a" a
        "label" label
        "active" #t
        "timer" 0.0
        "phase" (* (random) 6.28)))

;; 平面距离平方计算
(define (dist-sq x1 z1 x2 z2)
  (let ((dx (- x1 x2))
        (dz (- z1 z2)))
    (+ (* dx dx) (* dz dz))))

;; 保留最近 N 条记录
(define (take-recent lst n)
  (let loop ((l lst) (k n) (res '()))
    (if (or (null? l) (<= k 0))
        (reverse res)
        (loop (cdr l) (- k 1) (cons (car l) res)))))

;; 为玩家追加日志 (保留最新 8 条)
(define (player-add-log p msg)
  (let ((cur (hash-try-get p "logs")))
    (let ((lst (if cur cur '())))
      (hash-insert p "logs" (take-recent (cons msg lst) 8)))))

;; 全局广播消息至所有在线玩家
(define (broadcast-log msg)
  (let ((pids (hash-keys->list *players*)))
    (for-each
      (lambda (pid)
        (let ((p (hash-try-get *players* pid)))
          (if p
              (set! *players* (hash-insert *players* pid (player-add-log p msg))))))
      pids)))

;; --------------------------------------------------------
;; 1. 游戏初始化
;; --------------------------------------------------------
(define (init-game)
  (set! *players* (hash))
  (set! *user-entities* '())
  (set! *game-time* 0.0)

  ;; 基础建筑结构与核心信标
  (set! *entities*
    (list
      (make-entity "beacon-center" "sphere" 0.0 1.2 0.0 1.3 1.3 1.3 0.2 0.85 1.0 0.95 "Central Beacon")
      (make-entity "monolith-1" "cube" 12.0 2.0 12.0 2.0 4.0 2.0 0.95 0.25 0.25 1.0 "Red Monolith")
      (make-entity "monolith-2" "cube" -12.0 2.0 -12.0 2.0 4.0 2.0 0.25 0.95 0.35 1.0 "Green Monolith")
      (make-entity "monolith-3" "cube" 12.0 2.0 -12.0 2.0 4.0 2.0 0.95 0.85 0.2 1.0 "Gold Monolith")
      (make-entity "monolith-4" "cube" -12.0 2.0 12.0 2.0 4.0 2.0 0.7 0.3 0.95 1.0 "Purple Monolith")))

  ;; 散落在场景中的能量道具球 (生命球、加速球、积分能量核心)
  (set! *pickups*
    (list
      (make-pickup "pickup-hp-1" "heal" 6.0 0.8 6.0 0.7 0.7 0.7 0.1 1.0 0.5 0.95 "Health Orb (+30 HP)")
      (make-pickup "pickup-hp-2" "heal" -6.0 0.8 -6.0 0.7 0.7 0.7 0.1 1.0 0.5 0.95 "Health Orb (+30 HP)")
      (make-pickup "pickup-spd-1" "speed" -6.0 0.8 6.0 0.7 0.7 0.7 1.0 0.85 0.1 0.95 "Speed Boost (+40%)")
      (make-pickup "pickup-spd-2" "speed" 6.0 0.8 -6.0 0.7 0.7 0.7 1.0 0.85 0.1 0.95 "Speed Boost (+40%)")
      (make-pickup "pickup-core-1" "score" 0.0 0.8 10.0 0.8 0.8 0.8 0.3 0.75 1.0 0.95 "Energy Core (+50 Pts)")
      (make-pickup "pickup-core-2" "score" 0.0 0.8 -10.0 0.8 0.8 0.8 0.3 0.75 1.0 0.95 "Energy Core (+50 Pts)")
      (make-pickup "pickup-core-3" "score" 10.0 0.8 0.0 0.8 0.8 0.8 0.3 0.75 1.0 0.95 "Energy Core (+50 Pts)")
      (make-pickup "pickup-core-4" "score" -10.0 0.8 0.0 0.8 0.8 0.8 0.3 0.75 1.0 0.95 "Energy Core (+50 Pts)")))

  "Space Arena Initialized")

;; --------------------------------------------------------
;; 2. 玩家加入
;; --------------------------------------------------------
(define (on-player-join id)
  (let* ((spawn-x (- (* (random) 8.0) 4.0))
         (spawn-z (- (* (random) 8.0) 4.0))
         (pcount (length (hash-keys->list *players*)))
         (color-idx (modulo pcount (length *player-colors*)))
         (pcolor (list-ref *player-colors* color-idx))
         (new-player
           (hash "id" id
                 "x" spawn-x "y" 0.5 "z" spawn-z "yaw" 0.0
                 "hp" 100.0 "max-hp" 100.0 "score" 0 "speed-mult" 1.0
                 "color" pcolor
                 "logs" (list (string-append "Welcome Commander " id "! Type 'help' for commands.")))))
    (set! *players* (hash-insert *players* id new-player))
    (broadcast-log (string-append "Commander [" id "] connected to the sector.")))
  "joined")

;; --------------------------------------------------------
;; 3. 玩家断开
;; --------------------------------------------------------
(define (on-player-leave id)
  (set! *players* (hash-remove *players* id))
  (broadcast-log (string-append "Commander [" id "] disconnected from the sector."))
  "left")

;; --------------------------------------------------------
;; 道具拾取判定辅助逻辑
;; --------------------------------------------------------
(define (check-player-pickup id)
  (let ((p (hash-try-get *players* id)))
    (if p
        (let ((px (hash-get p "x"))
              (pz (hash-get p "z")))
          (set! *pickups*
            (map (lambda (pk)
                   (if (hash-get pk "active")
                       (let ((dist (dist-sq px pz (hash-get pk "x") (hash-get pk "z"))))
                         (if (<= dist 2.25) ;; 碰撞半径 1.5 (2.25 距离平方)
                             (let* ((ptype (hash-get pk "type"))
                                    (label (hash-get pk "label"))
                                    (updated-player
                                      (cond
                                        ((string=? ptype "heal")
                                         (let* ((cur-hp (hash-get p "hp"))
                                                (max-hp (hash-get p "max-hp"))
                                                (new-hp (min (+ cur-hp 30.0) max-hp))
                                                (new-score (+ (hash-get p "score") 15)))
                                           (player-add-log
                                             (hash-insert (hash-insert p "hp" new-hp) "score" new-score)
                                             (string-append "Acquired " label "! HP restored to " (number->string (round new-hp))))))

                                        ((string=? ptype "speed")
                                         (let* ((cur-mult (hash-get p "speed-mult"))
                                                (new-mult (min (+ cur-mult 0.4) 2.5))
                                                (new-score (+ (hash-get p "score") 25)))
                                           (player-add-log
                                             (hash-insert (hash-insert p "speed-mult" new-mult) "score" new-score)
                                             (string-append "Acquired " label "! Speed multiplier: " (number->string new-mult) "x"))))

                                        (else ;; score energy core
                                         (let ((new-score (+ (hash-get p "score") 50)))
                                           (player-add-log
                                             (hash-insert p "score" new-score)
                                             (string-append "Harvested " label "! Score +50")))))))
                               (set! *players* (hash-insert *players* id updated-player))
                               ;; 拾取后标记为未激活，进入 10 秒冷却
                               (hash-insert (hash-insert pk "active" #f) "timer" 10.0))
                             pk))
                       pk))
                 *pickups*))))))

;; --------------------------------------------------------
;; 4. 玩家移动 (结合速度倍率与视角偏航，触发即时拾取判定)
;; --------------------------------------------------------
(define (on-player-move id dx dz yaw)
  (let ((p (hash-try-get *players* id)))
    (if p
        (let* ((mult (hash-try-get p "speed-mult"))
               (speed (* 0.25 (if mult mult 1.0)))
               (new-x (+ (hash-get p "x") (* dx speed)))
               (new-z (+ (hash-get p "z") (* dz speed)))
               (updated (hash-insert (hash-insert (hash-insert p "x" new-x) "z" new-z) "yaw" yaw)))
          (set! *players* (hash-insert *players* id updated))
          (check-player-pickup id)
          "moved")
        "not-found")))

;; --------------------------------------------------------
;; 5. 丰富文本命令集 (help, heal, tp, speed, spawn, say, list, stats)
;; --------------------------------------------------------
(define (on-player-command id cmd)
  (let ((p (hash-try-get *players* id)))
    (if p
        (let* ((tokens (split-whitespace cmd))
               (verb (if (null? tokens) "" (car tokens)))
               (args (if (null? tokens) '() (cdr tokens))))
          (cond
            ;; 5.1 help: 显示所有可用命令
            ((string=? verb "help")
             (let ((msg "Available commands: help | heal | tp [x z] | speed <factor> | spawn <cube|sphere> <name> | say <text> | list | stats"))
               (set! *players* (hash-insert *players* id (player-add-log p msg)))
               msg))

            ;; 5.2 heal: 恢复 100% 生命值
            ((string=? verb "heal")
             (let* ((max-hp (hash-get p "max-hp"))
                    (updated (player-add-log (hash-insert p "hp" max-hp) "System: Health fully restored to 100%.")))
               (set! *players* (hash-insert *players* id updated))
               "System: Health restored to 100%."))

            ;; 5.3 tp [x z]: 传送回原点或指定坐标
            ((string=? verb "tp")
             (cond
               ((null? args)
                (let* ((updated (player-add-log (hash-insert (hash-insert p "x" 0.0) "z" 0.0)
                                                "System: Teleported to central beacon (0.0, 0.0).")))
                  (set! *players* (hash-insert *players* id updated))
                  (check-player-pickup id)
                  "System: Teleported to central beacon (0.0, 0.0)."))
               ((>= (length args) 2)
                (let ((tx (string->number (car args)))
                      (tz (string->number (cadr args))))
                  (if (and tx tz)
                      (let* ((msg (string-append "System: Teleported to (" (number->string tx) ", " (number->string tz) ")."))
                             (updated (player-add-log (hash-insert (hash-insert p "x" tx) "z" tz) msg)))
                        (set! *players* (hash-insert *players* id updated))
                        (check-player-pickup id)
                        msg)
                      "System: Invalid coordinates for tp. Usage: tp [x z]")))
               (else
                "System: Usage: tp [x z] (e.g. 'tp' or 'tp 10 -5')")))

            ;; 5.4 speed <factor>: 调整移动速度倍率
            ((string=? verb "speed")
             (if (not (null? args))
                 (let ((factor (string->number (car args))))
                   (if (and factor (> factor 0.0) (<= factor 5.0))
                       (let* ((msg (string-append "System: Movement speed multiplier set to " (number->string factor) "x."))
                              (updated (player-add-log (hash-insert p "speed-mult" factor) msg)))
                         (set! *players* (hash-insert *players* id updated))
                         msg)
                       "System: Speed multiplier must be a number between 0.1 and 5.0."))
                 "System: Usage: speed <factor> (e.g. 'speed 1.8')"))

            ;; 5.5 spawn <shape> <label>: 在当前坐标放置实体
            ((string=? verb "spawn")
             (if (>= (length args) 2)
                 (let ((shape (car args))
                       (label (string-join (cdr args) " ")))
                   (if (or (string=? shape "cube") (string=? shape "sphere"))
                       (let* ((ent-id (string-append "spawned-" id "-" (number->string (round (* (random) 100000.0)))))
                              (col (hash-get p "color"))
                              (new-ent (make-entity ent-id shape
                                                    (hash-get p "x") 1.0 (hash-get p "z")
                                                    1.2 1.2 1.2
                                                    (car col) (cadr col) (car (cdr (cdr col))) 0.9
                                                    label)))
                         (set! *user-entities* (cons new-ent *user-entities*))
                         (let* ((msg (string-append "System: Spawned " shape " [" label "] at your location."))
                                (updated (player-add-log p msg)))
                           (set! *players* (hash-insert *players* id updated))
                           msg))
                       "System: Supported shapes are 'cube' or 'sphere'."))
                 "System: Usage: spawn <cube|sphere> <label>"))

            ;; 5.6 say <text>: 广播文字至所有玩家日志列表
            ((string=? verb "say")
             (if (not (null? args))
                 (let* ((text (string-join args " "))
                        (chat (string-append "[Chat] " id ": " text)))
                   (broadcast-log chat)
                   (string-append "Broadcasted: " text))
                 "System: Usage: say <message>"))

            ;; 5.7 list: 列出所有在线玩家 ID 与坐标
            ((string=? verb "list")
             (let* ((pids (hash-keys->list *players*))
                    (info-list
                      (map (lambda (pid)
                             (let ((pl (hash-try-get *players* pid)))
                               (if pl
                                   (string-append pid " @ ("
                                                  (number->string (round (hash-get pl "x"))) ","
                                                  (number->string (round (hash-get pl "z"))) ")")
                                   pid)))
                           pids))
                    (msg (string-append "Online Pilots (" (number->string (length pids)) "): "
                                        (string-join info-list " | "))))
               (set! *players* (hash-insert *players* id (player-add-log p msg)))
               msg))

            ;; 5.8 stats: 显示自身详细属性
            ((string=? verb "stats")
             (let* ((msg (string-append "Status of [" id "]: HP: "
                                        (number->string (round (hash-get p "hp"))) "/"
                                        (number->string (round (hash-get p "max-hp")))
                                        " | Score: " (number->string (hash-get p "score"))
                                        " | Speed: " (number->string (hash-get p "speed-mult")) "x"
                                        " | Pos: (" (number->string (round (hash-get p "x"))) ","
                                        (number->string (round (hash-get p "z"))) ")"))
                    (updated (player-add-log p msg)))
               (set! *players* (hash-insert *players* id updated))
               msg))

            (else
             (let ((msg (string-append "Unknown command: '" cmd "'. Type 'help' for commands.")))
               (set! *players* (hash-insert *players* id (player-add-log p msg)))
               msg))))
        "player not found")))

;; --------------------------------------------------------
;; 6. 游戏时钟更新 (Tick) - 信标漂浮、能量球浮动、自转与重生物理
;; --------------------------------------------------------
(define (game-tick dt)
  (set! *game-time* (+ *game-time* dt))

  ;; 6.1 中心信标动态浮动与微缩放呼吸效果
  (set! *entities*
    (map (lambda (e)
           (if (string=? (hash-get e "id") "beacon-center")
               (let* ((bob-y (+ 1.2 (* 0.45 (sin (* *game-time* 2.0)))))
                      (pulse-s (+ 1.25 (* 0.12 (cos (* *game-time* 2.5))))))
                 (hash-insert (hash-insert (hash-insert (hash-insert e "y" bob-y)
                                                        "sx" pulse-s)
                                           "sy" pulse-s)
                              "sz" pulse-s))
               e))
         *entities*))

  ;; 6.2 拾取道具能量球更新 (浮动动画与冷却重生)
  (set! *pickups*
    (map (lambda (pk)
           (if (hash-get pk "active")
               ;; 激活状态：正弦波上下浮动
               (let* ((phase (hash-get pk "phase"))
                      (bob-y (+ (hash-get pk "base-y") (* 0.28 (sin (+ (* *game-time* 3.2) phase))))))
                 (hash-insert pk "y" bob-y))
               ;; 冷却状态：倒计时复活
               (let ((rem (- (hash-get pk "timer") dt)))
                 (if (<= rem 0.0)
                     (hash-insert (hash-insert pk "active" #t) "timer" 0.0)
                     (hash-insert pk "timer" rem)))))
         *pickups*))

  ;; 6.3 检查所有玩家与激活能量球的碰撞
  (for-each check-player-pickup (hash-keys->list *players*))
  #t)

;; --------------------------------------------------------
;; 7. 个性化客户端状态切片 (严格距离裁剪 + 视口内互相可见)
;; --------------------------------------------------------
(define (get-client-state-json id)
  (let ((p (hash-try-get *players* id)))
    (if p
        (let* ((px (hash-get p "x"))
               (pz (hash-get p "z"))
               (max-dist-sq (* *view-distance* *view-distance*))

               ;; (1) 筛选视野范围内的场景静态实体 (如信标、巨石柱)
               (visible-scene-ents
                 (filter (lambda (e)
                           (<= (dist-sq px pz (hash-get e "x") (hash-get e "z")) max-dist-sq))
                         *entities*))

               ;; (2) 筛选视野范围内的玩家生成实体
               (visible-user-ents
                 (filter (lambda (e)
                           (<= (dist-sq px pz (hash-get e "x") (hash-get e "z")) max-dist-sq))
                         *user-entities*))

               ;; (3) 筛选视野范围内的激活拾取道具，并转化为统一实体数据
               (visible-pickup-ents
                 (map (lambda (pk)
                        (hash "id" (hash-get pk "id")
                              "shape" "sphere"
                              "x" (hash-get pk "x")
                              "y" (hash-get pk "y")
                              "z" (hash-get pk "z")
                              "sx" (hash-get pk "sx")
                              "sy" (hash-get pk "sy")
                              "sz" (hash-get pk "sz")
                              "r" (hash-get pk "r")
                              "g" (hash-get pk "g")
                              "b" (hash-get pk "b")
                              "a" (hash-get pk "a")
                              "label" (hash-get pk "label")))
                      (filter (lambda (pk)
                                (and (hash-get pk "active")
                                     (<= (dist-sq px pz (hash-get pk "x") (hash-get pk "z")) max-dist-sq)))
                              *pickups*)))

               ;; (4) 关键机制：视野范围内互相可见！
               ;; 将其他在线玩家作为实体 (shape=cube, label=玩家ID, 专属颜色) 注入该玩家视口
               (visible-other-players
                 (let ((other-pids (filter (lambda (pid) (not (string=? pid id)))
                                           (hash-keys->list *players*))))
                   (map (lambda (opid)
                          (let* ((op (hash-get *players* opid))
                                 (col (hash-get op "color")))
                            (hash "id" (string-append "player-" opid)
                                  "shape" "cube"
                                  "x" (hash-get op "x")
                                  "y" (hash-get op "y")
                                  "z" (hash-get op "z")
                                  "sx" 0.8 "sy" 1.6 "sz" 0.8
                                  "r" (car col)
                                  "g" (cadr col)
                                  "b" (car (cdr (cdr col)))
                                  "a" (car (cdr (cdr (cdr col))))
                                  "label" opid)))
                        (filter (lambda (opid)
                                  (let ((op (hash-get *players* opid)))
                                    (<= (dist-sq px pz (hash-get op "x") (hash-get op "z")) max-dist-sq)))
                                other-pids))))

               ;; 拼接所有可见实体
               (all-visible-ents
                 (append visible-scene-ents
                         visible-user-ents
                         visible-pickup-ents
                         visible-other-players))

               ;; 序列化实体为 JSON 图元列表
               (entities-json
                 (string-join
                   (map (lambda (e)
                          (string-append
                            "{\"id\":\"" (hash-get e "id") "\","
                            "\"shape\":\"" (hash-get e "shape") "\","
                            "\"pos\":[" (number->string (hash-get e "x")) ","
                                        (number->string (hash-get e "y")) ","
                                        (number->string (hash-get e "z")) "],"
                            "\"size\":[" (number->string (hash-get e "sx")) ","
                                         (number->string (hash-get e "sy")) ","
                                         (number->string (hash-get e "sz")) "],"
                            "\"color\":[" (number->string (hash-get e "r")) ","
                                          (number->string (hash-get e "g")) ","
                                          (number->string (hash-get e "b")) ","
                                          (number->string (hash-get e "a")) "],"
                            "\"label\":\"" (hash-get e "label") "\"}"))
                        all-visible-ents)
                   ","))

               ;; 序列化玩家的近期日志
               (player-logs (hash-try-get p "logs"))
               (logs-json
                 (if (and player-logs (not (null? player-logs)))
                     (string-join
                       (map (lambda (m)
                              ;; 简单替换双引号避免 JSON 解析异常
                              (let ((safe-m (string-join (split-whitespace m) " ")))
                                (string-append "\"" safe-m "\"")))
                            player-logs)
                       ",")
                     "\"Sector Normal\"")))

          ;; 输出符合 Rust shared::ClientState 定义的完整 JSON 包
          (string-append
            "{"
            "\"client_id\":\"" id "\","
            "\"camera_target\":[" (number->string (hash-get p "x")) ","
                                  (number->string (hash-get p "y")) ","
                                  (number->string (hash-get p "z")) "],"
            "\"camera_yaw\":" (number->string (hash-get p "yaw")) ","
            "\"hp\":" (number->string (hash-get p "hp")) ","
            "\"max_hp\":" (number->string (hash-get p "max-hp")) ","
            "\"score\":" (number->string (hash-get p "score")) ","
            "\"entities\":[" entities-json "],"
            "\"logs\":[" logs-json "]"
            "}"))
        "{}")))
