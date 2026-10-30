;; ========================================================
;; games/tank_battle_2d/server/main.scm
;; Cyber Tank Battle 2D - Authoritative Server Simulation
;; ========================================================

;; 玩家哈希表: id -> player-hash
(define *players* (hash))

;; 场景静态掩体与障碍物 (碉堡、能量柱、防御墙)
(define *obstacles* '())

;; 场景动态敌方战术无人机/巡逻坦克 (AI 实体)
(define *drones* '())

;; 场景可拾取能量与维修道具 (纳米修复、电浆充能、能量护盾、EMP核心)
(define *pickups* '())

;; 玩家与敌机发射的高能电浆炮弹列表
(define *bullets* '())

;; 玩家通过指令动态生成的战术实体
(define *user-entities* '())

;; 全局仿真累积时钟 (秒)
(define *game-time* 0.0)

;; 空间视距裁剪常量 (米) - 严格 25.0 米视距
(define *view-distance* 25.0)

;; 战车阵营调色板 (RGBA)
(define *tank-colors*
  '((0.2 0.9 0.5 1.0)    ;; 极光绿 (Neon Aurora Green - 本地战车预设)
    (0.2 0.8 1.0 1.0)    ;; 赛博青 (Cyber Cyan)
    (1.0 0.3 0.3 1.0)    ;; 猩红先锋 (Crimson Red)
    (1.0 0.8 0.1 1.0)    ;; 琥珀金光 (Amber Gold)
    (0.8 0.4 1.0 1.0)    ;; 灵能离子紫 (Plasma Purple)
    (1.0 0.5 0.1 1.0)    ;; 太阳耀斑橙 (Solar Flare Orange)
    (0.2 1.0 0.9 1.0)    ;; 冰晶电弧蓝 (Frost Arc Blue)
    (1.0 0.3 0.7 1.0)))  ;; 霓虹玫红 (Neon Magenta)

;; --------------------------------------------------------
;; 基础数学与辅助工具
;; --------------------------------------------------------

;; 两点水平欧氏距离平方
(define (dist-sq x1 z1 x2 z2)
  (let ((dx (- x1 x2))
        (dz (- z1 z2)))
    (+ (* dx dx) (* dz dz))))

;; 创建场景静态掩体实体
(define (make-obstacle id x z sx sz r g b a label)
  (hash "id" id "shape" "cube"
        "x" x "y" 0.6 "z" z
        "sx" sx "sy" 1.2 "sz" sz
        "r" r "g" g "b" b "a" a
        "label" label))

;; 创建敌方无人机/巡逻坦克
(define (make-drone id dtype x z hp max-hp speed r g b a label)
  (hash "id" id "type" dtype
        "x" x "y" 0.5 "z" z
        "vx" (* speed (if (> (random) 0.5) 1.0 -1.0))
        "vz" (* speed (if (> (random) 0.5) 1.0 -1.0))
        "hp" hp "max-hp" max-hp
        "speed" speed
        "sx" 1.2 "sy" 0.8 "sz" 1.2
        "r" r "g" g "b" b "a" a
        "label" label
        "active" #t
        "timer" 0.0
        "ai-timer" 0.0))

;; 创建战场拾取道具
(define (make-pickup id ptype x z r g b a label)
  (hash "id" id "type" ptype
        "x" x "base-y" 0.6 "y" 0.6 "z" z
        "sx" 0.8 "sy" 0.8 "sz" 0.8
        "r" r "g" g "b" b "a" a
        "label" label
        "active" #t
        "timer" 0.0
        "phase" (* (random) 6.28)))

;; 安全获取最近 N 条日志
(define (take-recent lst n)
  (let loop ((l lst) (k n) (res '()))
    (if (or (null? l) (<= k 0))
        (reverse res)
        (loop (cdr l) (- k 1) (cons (car l) res)))))

;; 为指定玩家添加战场日志 (保持最新 6 条)
(define (player-add-log p msg)
  (let ((cur (hash-try-get p "logs")))
    (let ((lst (if cur cur '())))
      (hash-insert p "logs" (take-recent (cons msg lst) 6)))))

;; 全服广播战报日志
(define (broadcast-log msg)
  (let ((pids (hash-keys->list *players*)))
    (for-each
      (lambda (pid)
        (let ((p (hash-try-get *players* pid)))
          (if (and p (hash? p))
              (set! *players* (hash-insert *players* pid (player-add-log p msg))))))
      pids)))

;; --------------------------------------------------------
;; 1. 游戏初始化 (init-game)
;; --------------------------------------------------------
(define (init-game)
  (displayln "[Server] Initializing Cyber Tank Battle 2D world...")
  (set! *players* (hash))
  (set! *bullets* '())
  (set! *user-entities* '())
  (set! *game-time* 0.0)

  ;; 1.1 初始化四座角部重装掩体与中央能量塔
  (set! *obstacles*
    (list
      (make-obstacle "bunker-center" 0.0 0.0 2.2 2.2 0.2 0.7 0.9 0.95 "Control Tower")
      (make-obstacle "bunker-nw" -12.0 -12.0 2.8 2.8 0.4 0.5 0.6 0.95 "NW Armored Bunker")
      (make-obstacle "bunker-ne"  12.0 -12.0 2.8 2.8 0.4 0.5 0.6 0.95 "NE Armored Bunker")
      (make-obstacle "bunker-sw" -12.0  12.0 2.8 2.8 0.4 0.5 0.6 0.95 "SW Armored Bunker")
      (make-obstacle "bunker-se"  12.0  12.0 2.8 2.8 0.4 0.5 0.6 0.95 "SE Armored Bunker")
      (make-obstacle "wall-n"   0.0 -18.0 8.0 1.2 0.3 0.4 0.5 0.9 "North Energy Barrier")
      (make-obstacle "wall-s"   0.0  18.0 8.0 1.2 0.3 0.4 0.5 0.9 "South Energy Barrier")))

  ;; 1.2 初始化敌方自主 AI 巡逻无人机与重装战车
  (set! *drones*
    (list
      (make-drone "drone-scout-1" "scout" -8.0 -5.0 40.0 40.0 2.2 1.0 0.25 0.25 0.95 "Scout Drone Alpha")
      (make-drone "drone-scout-2" "scout"  8.0  5.0 40.0 40.0 2.2 1.0 0.25 0.25 0.95 "Scout Drone Beta")
      (make-drone "drone-patrol-1" "assault" -5.0  9.0 80.0 80.0 1.5 1.0 0.4 0.1 0.95 "Assault Drone Gamma")
      (make-drone "drone-patrol-2" "assault"  5.0 -9.0 80.0 80.0 1.5 1.0 0.4 0.1 0.95 "Assault Drone Delta")
      (make-drone "drone-boss-titan" "boss" 0.0 -10.0 200.0 200.0 0.9 0.9 0.1 0.3 1.0 "Titan Siege Tank [Boss]")))

  ;; 1.3 初始化散布在战场的战术道具球
  (set! *pickups*
    (list
      (make-pickup "pickup-repair-1" "repair" -6.0 -6.0 0.1 1.0 0.4 0.95 "Nano-Repair (+35 HP)")
      (make-pickup "pickup-repair-2" "repair"  6.0  6.0 0.1 1.0 0.4 0.95 "Nano-Repair (+35 HP)")
      (make-pickup "pickup-overcharge-1" "overcharge" 0.0 7.0 1.0 0.85 0.1 0.95 "Plasma Overcharge (+50 Pts)")
      (make-pickup "pickup-overcharge-2" "overcharge" 0.0 -7.0 1.0 0.85 0.1 0.95 "Plasma Overcharge (+50 Pts)")
      (make-pickup "pickup-shield-1" "shield" -8.0 8.0 0.2 0.85 1.0 0.95 "Shield Battery (+40 Shield)")
      (make-pickup "pickup-emp-1" "emp" 8.0 -8.0 0.85 0.3 1.0 0.95 "Tactical EMP Core (+100 Pts)")))

  "Cyber Tank Battle 2D Initialized")

;; --------------------------------------------------------
;; 2. 玩家生命周期 (Join / Leave)
;; --------------------------------------------------------
(define (on-player-join id)
  (displayln (string-append "[Server] Tank Commander joined: " id))
  (let* ((spawn-x (- (* (random) 6.0) 3.0))
         (spawn-z (- (* (random) 6.0) 3.0))
         (pcount (length (hash-keys->list *players*)))
         (color-idx (modulo pcount (length *tank-colors*)))
         (pcolor (list-ref *tank-colors* color-idx))
         (new-player
           (hash "id" id
                 "x" spawn-x "y" 0.5 "z" spawn-z "yaw" 0.0
                 "hp" 100.0 "max-hp" 100.0
                 "shield" 50.0 "max-shield" 50.0
                 "score" 0 "kills" 0
                 "speed-mult" 1.0
                 "color" pcolor
                 "logs" (list (string-append "Welcome Commander " id "! Press [ENTER] for commands, [WASD] to drive tank.")))))
    (set! *players* (hash-insert *players* id new-player))
    (broadcast-log (string-append "Commander [" id "] deployed battle tank to arena.")))
  "joined")

(define (on-player-leave id)
  (displayln (string-append "[Server] Tank Commander departed: " id))
  (set! *players* (hash-remove *players* id))
  (broadcast-log (string-append "Commander [" id "] retreated from arena."))
  "left")

;; --------------------------------------------------------
;; 3. 道具拾取与碰撞判定 (check-player-pickup)
;; --------------------------------------------------------
(define (check-player-pickup id)
  (let ((p (hash-try-get *players* id)))
    (if (and p (hash? p))
        (let ((px (hash-get p "x"))
              (pz (hash-get p "z")))
          (set! *pickups*
            (map (lambda (pk)
                   (if (hash-get pk "active")
                       (let ((dist (dist-sq px pz (hash-get pk "x") (hash-get pk "z"))))
                         (if (<= dist 2.25) ;; 拾取碰撞半径 1.5 米 (dist-sq <= 2.25)
                             (let* ((ptype (hash-get pk "type"))
                                    (label (hash-get pk "label"))
                                    (updated-player
                                      (cond
                                        ;; 纳米修复球: 恢复 35 HP + 15 分
                                        ((string=? ptype "repair")
                                         (let* ((cur-hp (hash-get p "hp"))
                                                (max-hp (hash-get p "max-hp"))
                                                (new-hp (min (+ cur-hp 35.0) max-hp))
                                                (new-score (+ (hash-get p "score") 15)))
                                           (player-add-log
                                             (hash-insert (hash-insert p "hp" new-hp) "score" new-score)
                                             (string-append "Harvested " label "! Hull integrity: " (number->string (round new-hp)) "%"))))

                                        ;; 电浆充能球: 加速 20% + 50 分
                                        ((string=? ptype "overcharge")
                                         (let* ((cur-mult (hash-get p "speed-mult"))
                                                (new-mult (min (+ cur-mult 0.25) 2.2))
                                                (new-score (+ (hash-get p "score") 50)))
                                           (player-add-log
                                             (hash-insert (hash-insert p "speed-mult" new-mult) "score" new-score)
                                             (string-append "Harvested " label "! Engine speed: " (number->string new-mult) "x"))))

                                        ;; 能量护盾球: 补充 40 点护盾 + 25 分
                                        ((string=? ptype "shield")
                                         (let* ((cur-sh (hash-get p "shield"))
                                                (max-sh (hash-get p "max-shield"))
                                                (new-sh (min (+ cur-sh 40.0) max-sh))
                                                (new-score (+ (hash-get p "score") 25)))
                                           (player-add-log
                                             (hash-insert (hash-insert p "shield" new-sh) "score" new-score)
                                             (string-append "Harvested " label "! Shields at " (number->string (round new-sh)) " SP"))))

                                        ;; EMP 核心: +100 分并释放局部冲击波
                                        (else
                                         (let ((new-score (+ (hash-get p "score") 100)))
                                           (player-add-log
                                             (hash-insert p "score" new-score)
                                             (string-append "Detonated " label "! Score +100")))))))
                               (set! *players* (hash-insert *players* id updated-player))
                               ;; 拾取后标记未激活，进入 10 秒重生倒计时
                               (hash-insert (hash-insert pk "active" #f) "timer" 10.0))
                             pk))
                       pk))
                 *pickups*))))))

;; --------------------------------------------------------
;; 4. 玩家位移 (on-player-move)
;; --------------------------------------------------------
(define (on-player-move id dx dz yaw)
  (let ((p (hash-try-get *players* id)))
    (if (and p (hash? p))
        (let* ((mult (hash-try-get p "speed-mult"))
               (base-step 0.25)
               (speed (* base-step (if mult mult 1.0)))
               (target-x (+ (hash-get p "x") (* dx speed)))
               (target-z (+ (hash-get p "z") (* dz speed)))
               ;; 战场边界限制在 -24.0 ~ 24.0 米之间
               (clamped-x (max -24.0 (min 24.0 target-x)))
               (clamped-z (max -24.0 (min 24.0 target-z)))
               (updated (hash-insert (hash-insert (hash-insert p "x" clamped-x) "z" clamped-z) "yaw" yaw)))
          (set! *players* (hash-insert *players* id updated))
          (check-player-pickup id)
          "moved")
        "not-found")))

;; --------------------------------------------------------
;; 5. 开火发射电浆炮弹 (spawn-bullet)
;; --------------------------------------------------------
(define (spawn-bullet owner-id px pz yaw)
  (let* ((speed 18.0)
         ;; yaw=0 为向后(+Z), 结合客户端朝向计算航向向量
         ;; sin(yaw) 分量对应 X, cos(yaw) 对应 Z 方向
         (vx (* (sin yaw) speed))
         (vz (* (cos yaw) speed -1.0))
         (bid (string-append "bullet-" owner-id "-" (number->string (round (* *game-time* 100.0)))))
         (b (hash "id" bid
                  "owner" owner-id
                  "x" px "y" 0.5 "z" pz
                  "vx" vx "vz" vz
                  "life" 2.2)))
    (set! *bullets* (cons b *bullets*))
    bid))

;; --------------------------------------------------------
;; 6. 20 TPS 世界仿真主时钟 (game-tick / tick-game)
;; --------------------------------------------------------
(define (game-tick dt)
  (set! *game-time* (+ *game-time* dt))

  ;; 6.1 推进电浆炮弹飞行与碰撞检测
  (let ((surviving-bullets '()))
    (for-each
      (lambda (b)
        (let* ((rem-life (- (hash-get b "life") dt))
               (nx (+ (hash-get b "x") (* (hash-get b "vx") dt)))
               (nz (+ (hash-get b "z") (* (hash-get b "vz") dt)))
               (owner (hash-get b "owner"))
               (hit #f))
          (if (> rem-life 0.0)
              ;; 检查炮弹对无人机的碰撞 (半径 1.6 米)
              (begin
                (set! *drones*
                  (map (lambda (d)
                         (if (and (not hit)
                                  (hash-get d "active")
                                  (<= (dist-sq nx nz (hash-get d "x") (hash-get d "z")) 2.56))
                             (let* ((cur-hp (hash-get d "hp"))
                                    (new-hp (- cur-hp 35.0)))
                               (set! hit #t)
                               (if (<= new-hp 0.0)
                                   ;; 击毁敌机：加分与击杀统计
                                   (begin
                                     (let ((p (hash-try-get *players* owner)))
                                       (if (and p (hash? p))
                                           (let* ((score-gain (if (string=? (hash-get d "type") "boss") 300 80))
                                                  (new-score (+ (hash-get p "score") score-gain))
                                                  (new-kills (+ (hash-get p "kills") 1))
                                                  (up-p (player-add-log
                                                          (hash-insert (hash-insert p "score" new-score) "kills" new-kills)
                                                          (string-append "TARGET DESTROYED: " (hash-get d "label") "! (+" (number->string score-gain) " Pts)"))))
                                             (set! *players* (hash-insert *players* owner up-p)))))
                                     (broadcast-log (string-append "ALERT: " (hash-get d "label") " neutralized by Commander [" owner "]!"))
                                     ;; 击毁后进入 12 秒重生冷却
                                     (hash-insert (hash-insert d "active" #f) "timer" 12.0))
                                   ;; 未击毁扣血
                                   (hash-insert d "hp" new-hp)))
                             d))
                       *drones*))
                (if (not hit)
                    (set! surviving-bullets
                      (cons (hash-insert (hash-insert (hash-insert b "x" nx) "z" nz) "life" rem-life)
                            surviving-bullets))))
              #f)))
      *bullets*)
    (set! *bullets* surviving-bullets))

  ;; 6.2 敌方战术无人机 AI 巡逻、避障与重生倒计时
  (set! *drones*
    (map (lambda (d)
           (if (hash-get d "active")
               (let* ((dx (+ (hash-get d "x") (* (hash-get d "vx") dt)))
                      (dz (+ (hash-get d "z") (* (hash-get d "vz") dt)))
                      (vx (hash-get d "vx"))
                      (vz (hash-get d "vz"))
                      ;; 边界反弹判定
                      (new-vx (if (or (<= dx -20.0) (>= dx 20.0)) (* vx -1.0) vx))
                      (new-vz (if (or (<= dz -20.0) (>= dz 20.0)) (* vz -1.0) vz))
                      (clamped-x (max -20.0 (min 20.0 dx)))
                      (clamped-z (max -20.0 (min 20.0 dz))))
                 (hash-insert (hash-insert (hash-insert (hash-insert d "x" clamped-x) "z" clamped-z) "vx" new-vx) "vz" new-vz))
               ;; 冷却中推进重生倒计时
               (let ((rem (- (hash-get d "timer") dt)))
                 (if (<= rem 0.0)
                     (hash-insert (hash-insert (hash-insert d "active" #t) "hp" (hash-get d "max-hp")) "timer" 0.0)
                     (hash-insert d "timer" rem)))))
         *drones*))

  ;; 6.3 道具球正弦波悬浮呼吸灯与重生时钟
  (set! *pickups*
    (map (lambda (pk)
           (if (hash-get pk "active")
               (let* ((phase (hash-get pk "phase"))
                      (bob-y (+ (hash-get pk "base-y") (* 0.22 (sin (+ (* *game-time* 3.6) phase))))))
                 (hash-insert pk "y" bob-y))
               (let ((rem (- (hash-get pk "timer") dt)))
                 (if (<= rem 0.0)
                     (hash-insert (hash-insert pk "active" #t) "timer" 0.0)
                     (hash-insert pk "timer" rem)))))
         *pickups*))

  ;; 6.4 遍历在线玩家做即时拾取判定
  (for-each check-player-pickup (hash-keys->list *players*))
  #t)

;; --------------------------------------------------------
;; 7. 自定义终端指令系统 (on-player-command)
;; --------------------------------------------------------
(define (on-player-command id cmd)
  (let ((p (hash-try-get *players* id)))
    (if (and p (hash? p))
        (let* ((tokens (split-whitespace cmd))
               (verb (if (null? tokens) "" (car tokens)))
               (args (if (null? tokens) '() (cdr tokens))))
          (cond
            ;; 7.1 help: 列出全部可用战场指令
            ((string=? verb "help")
             (let ((msg "Available commands: help | stats | fire | heal | shield | speed <val> | tp [x z] | spawn <shape> <name> | bomb | say <text> | list"))
               (set! *players* (hash-insert *players* id (player-add-log p msg)))
               msg))

            ;; 7.2 stats: 输出战车属性与雷达数据
            ((string=? verb "stats")
             (let ((msg (string-append
                          "Commander [" id "] | HP: " (number->string (round (hash-get p "hp"))) "/" (number->string (round (hash-get p "max-hp")))
                          " | Shield: " (number->string (round (hash-get p "shield")))
                          " | Score: " (number->string (hash-get p "score"))
                          " | Kills: " (number->string (hash-get p "kills"))
                          " | Speed: " (number->string (hash-get p "speed-mult")) "x")))
               (set! *players* (hash-insert *players* id (player-add-log p msg)))
               msg))

            ;; 7.3 fire: 战车发射主炮电浆炮弹
            ((string=? verb "fire")
             (let ((bid (spawn-bullet id (hash-get p "x") (hash-get p "z") (hash-get p "yaw"))))
               (set! *players* (hash-insert *players* id (player-add-log p "Fired high-velocity plasma round!")))
               "Fired plasma cannon."))

            ;; 7.4 heal: 战车纳米装甲与护盾 100% 修复
            ((string=? verb "heal")
             (let* ((up-p (hash-insert (hash-insert p "hp" (hash-get p "max-hp")) "shield" (hash-get p "max-shield")))
                    (final-p (player-add-log up-p "Hull integrity and energy shields restored to 100%.")))
               (set! *players* (hash-insert *players* id final-p))
               "Health restored to 100%."))

            ;; 7.5 shield: 激活紧急能量过载偏折护盾
            ((string=? verb "shield")
             (let* ((up-p (hash-insert p "shield" 50.0))
                    (final-p (player-add-log up-p "Tactical deflector shield fully charged.")))
               (set! *players* (hash-insert *players* id final-p))
               "Deflector shield fully charged."))

            ;; 7.6 speed <factor>: 设定引擎油门倍率
            ((string=? verb "speed")
             (if (null? args)
                 "Usage: speed <multiplier> (e.g. speed 1.5)"
                 (let* ((factor (string->number (car args)))
                        (spd (if factor (max 0.2 (min 3.0 factor)) 1.0))
                        (up-p (hash-insert p "speed-mult" spd))
                        (msg (string-append "Engine throttle multiplier set to " (number->string spd) "x."))
                        (final-p (player-add-log up-p msg)))
                   (set! *players* (hash-insert *players* id final-p))
                   msg)))

            ;; 7.7 tp [x z]: 战术折跃传送
            ((string=? verb "tp")
             (let* ((tx (if (and (not (null? args)) (not (null? (cdr args))))
                            (let ((n (string->number (car args)))) (if n n 0.0))
                            0.0))
                    (tz (if (and (not (null? args)) (not (null? (cdr args))))
                            (let ((n (string->number (cadr args)))) (if n n 0.0))
                            0.0))
                    (up-p (hash-insert (hash-insert p "x" tx) "z" tz))
                    (msg (string-append "Teleported to (" (number->string tx) ", " (number->string tz) ")."))
                    (final-p (player-add-log up-p msg)))
               (set! *players* (hash-insert *players* id final-p))
               (check-player-pickup id)
               msg))

            ;; 7.8 spawn <shape> <name>: 部署战场掩体或战术信标
            ((string=? verb "spawn")
             (if (null? args)
                 "Usage: spawn <cube|sphere|wall> <name>"
                 (let* ((shape (car args))
                        (label (if (null? (cdr args)) "Tactical Unit" (string-join (cdr args) " ")))
                        (px (hash-get p "x"))
                        (pz (hash-get p "z"))
                        (sx (+ px (* 2.0 (sin (hash-get p "yaw")))))
                        (sz (+ pz (* 2.0 (cos (hash-get p "yaw")) -1.0)))
                        (eid (string-append "spawned-" id "-" (number->string (round (* *game-time* 100.0)))))
                        (new-ent (hash "id" eid "shape" shape
                                       "x" sx "y" 0.6 "z" sz
                                       "sx" 1.4 "sy" 1.4 "sz" 1.4
                                       "r" 0.3 "g" 0.8 "b" 1.0 "a" 0.95
                                       "label" label)))
                   (set! *user-entities* (cons new-ent *user-entities*))
                   (let ((msg (string-append "Spawned " shape " [" label "] at sector.")))
                     (set! *players* (hash-insert *players* id (player-add-log p msg)))
                     msg))))

            ;; 7.9 bomb: 引爆全域 EMP 战术核脉冲
            ((string=? verb "bomb")
             (let* ((px (hash-get p "x"))
                    (pz (hash-get p "z"))
                    (cleared 0))
               (set! *drones*
                 (map (lambda (d)
                        (if (and (hash-get d "active")
                                 (<= (dist-sq px pz (hash-get d "x") (hash-get d "z")) 225.0)) ;; 15m 爆炸半径
                            (begin
                              (set! cleared (+ cleared 1))
                              (hash-insert (hash-insert d "active" #f) "timer" 14.0))
                            d))
                      *drones*))
               (let* ((score-gain (* cleared 100))
                      (new-score (+ (hash-get p "score") score-gain))
                      (new-kills (+ (hash-get p "kills") cleared))
                      (msg (string-append "EMP DETONATED! Neutralized " (number->string cleared) " hostile units (+" (number->string score-gain) " Pts)."))
                      (final-p (player-add-log (hash-insert (hash-insert p "score" new-score) "kills" new-kills) msg)))
                 (set! *players* (hash-insert *players* id final-p))
                 (broadcast-log (string-append "TACTICAL ALERT: Commander [" id "] detonated arena EMP wave!"))
                 msg)))

            ;; 7.10 say <text>: 战场全频道无线电广播
            ((string=? verb "say")
             (if (null? args)
                 "Usage: say <message>"
                 (let* ((text (string-join args " "))
                        (msg (string-append id ": " text)))
                   (broadcast-log msg)
                   "Broadcasted radio transmission to sector.")))

            ;; 7.11 list: 列出当前在线战车指挥官列表
            ((string=? verb "list")
             (let* ((pids (hash-keys->list *players*))
                    (info-list
                      (map (lambda (pid)
                             (let ((pl (hash-get *players* pid)))
                               (string-append pid " [Score: " (number->string (hash-get pl "score")) "]")))
                           pids)))
               (string-append "Active Tank Commanders (" (number->string (length pids)) "): "
                              (string-join info-list " | "))))

            (else (string-append "Echo: " cmd))))
        "Player not found")))

;; --------------------------------------------------------
;; 8. RemoteEvent 双端离散 RPC 事件处理
;; --------------------------------------------------------
(define (on-remote-event player event payload)
  (cond
    ((string=? event "fire")
     (let ((p (hash-try-get *players* player)))
       (if (and p (hash? p))
           (spawn-bullet player (hash-get p "x") (hash-get p "z") (hash-get p "yaw"))
           #f)))
    ((string=? event "ping")
     (fire-client! player "pong" "{\"status\":\"active\"}"))
    (else #t)))

;; --------------------------------------------------------
;; 9. 25m 空间视距快照 JSON 序列化 (get-client-state-json)
;; --------------------------------------------------------
(define (get-client-state-json id)
  (let ((p (hash-try-get *players* id)))
    (if (and p (hash? p))
        (let* ((px (hash-get p "x"))
               (pz (hash-get p "z"))
               (max-dist-sq (* *view-distance* *view-distance*))

               ;; (1) 筛选 25m 视距内的静态掩体障碍物
               (visible-obstacles
                 (filter (lambda (obs)
                           (<= (dist-sq px pz (hash-get obs "x") (hash-get obs "z")) max-dist-sq))
                         *obstacles*))

               ;; (2) 筛选 25m 视距内的激活敌方无人机/坦克
               (visible-drones
                 (map (lambda (d)
                        (hash "id" (hash-get d "id")
                              "shape" (string-append "drone-" (hash-get d "type"))
                              "x" (hash-get d "x")
                              "y" (hash-get d "y")
                              "z" (hash-get d "z")
                              "sx" (hash-get d "sx")
                              "sy" (hash-get d "sy")
                              "sz" (hash-get d "sz")
                              "r" (hash-get d "r")
                              "g" (hash-get d "g")
                              "b" (hash-get d "b")
                              "a" (hash-get d "a")
                              "label" (string-append (hash-get d "label") " [" (number->string (round (hash-get d "hp"))) " HP]")))
                      (filter (lambda (d)
                                (and (hash-get d "active")
                                     (<= (dist-sq px pz (hash-get d "x") (hash-get d "z")) max-dist-sq)))
                              *drones*)))

               ;; (3) 筛选 25m 视距内的激活道具球
               (visible-pickups
                 (map (lambda (pk)
                        (hash "id" (hash-get pk "id")
                              "shape" "pickup"
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

               ;; (4) 筛选 25m 视距内的高速电浆炮弹
               (visible-bullets
                 (map (lambda (b)
                        (hash "id" (hash-get b "id")
                              "shape" "bullet"
                              "x" (hash-get b "x")
                              "y" (hash-get b "y")
                              "z" (hash-get b "z")
                              "sx" 0.35 "sy" 0.35 "sz" 0.35
                              "r" 1.0 "g" 0.85 "b" 0.2 "a" 1.0
                              "label" ""))
                      (filter (lambda (b)
                                (<= (dist-sq px pz (hash-get b "x") (hash-get b "z")) max-dist-sq))
                              *bullets*)))

               ;; (5) 筛选 25m 视距内的玩家自定义实体
               (visible-user-ents
                 (filter (lambda (e)
                           (<= (dist-sq px pz (hash-get e "x") (hash-get e "z")) max-dist-sq))
                         *user-entities*))

               ;; (6) 核心机制：同服多玩家 25m 互相可见！
               ;; 将其他在线玩家包装为实体 (shape=cube/tank, label=玩家ID, 阵营颜色)
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
                                  "sx" 1.4 "sy" 1.0 "sz" 1.4
                                  "r" (car col)
                                  "g" (cadr col)
                                  "b" (car (cdr (cdr col)))
                                  "a" (car (cdr (cdr (cdr col))))
                                  "label" opid)))
                        (filter (lambda (opid)
                                  (let ((op (hash-get *players* opid)))
                                    (<= (dist-sq px pz (hash-get op "x") (hash-get op "z")) max-dist-sq)))
                                other-pids))))

               ;; 整合所有视距内实体
               (all-ents (append visible-obstacles
                                 visible-drones
                                 visible-pickups
                                 visible-bullets
                                 visible-user-ents
                                 visible-other-players))

               ;; 序列化实体列表为 JSON 字符串
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
                        all-ents)
                   ","))

               ;; 序列化战地通信日志
               (player-logs (hash-try-get p "logs"))
               (logs-json
                 (if (and player-logs (not (null? player-logs)))
                     (string-join
                       (map (lambda (m)
                              (let ((safe-m (string-join (split-whitespace m) " ")))
                                (string-append "\"" safe-m "\"")))
                            player-logs)
                       ",")
                     "\"Sector Radar Online\"")))

          ;; 输出严谨的 ClientState JSON 包
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
