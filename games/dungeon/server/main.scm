;; ========================================================
;; games/dungeon.scm - LispCraft 暗黑地牢探险模式
;; ========================================================

;; 玩家哈希表: id -> player-hash
(define *players* (hash))

;; 地牢静态结构 (核心祭坛、远古石柱、基础火把)
(define *entities* '())

;; 冒险者放置的动态火把与实体
(define *user-entities* '())

;; 地牢散落的可拾取宝物与药剂
(define *pickups* '())

;; 全局游戏时钟累积时间 (秒)
(define *game-time* 0.0)

;; 地牢环境视口裁剪视野距离 (地牢光线昏暗，视野为 20.0)
(define *view-distance* 20.0)

;; 冒险者斗篷与灵魂光晕颜色 (RGBA)
(define *player-colors*
  '((0.85 0.35 0.15 1.0)  ;; 火焰橙
    (0.3 0.65 1.0 1.0)   ;; 冰霜蓝
    (0.4 0.9 0.45 1.0)   ;; 剧毒翠绿
    (0.85 0.75 0.25 1.0) ;; 圣光金
    (0.75 0.3 0.9 1.0)   ;; 虚空紫
    (0.9 0.2 0.35 1.0))) ;; 鲜血红

;; --------------------------------------------------------
;; 基础辅助函数
;; --------------------------------------------------------

(define (make-entity id shape x y z sx sy sz r g b a label)
  (hash "id" id "shape" shape "x" x "y" y "z" z
        "sx" sx "sy" sy "sz" sz
        "r" r "g" g "b" b "a" a
        "label" label))

(define (make-pickup id ptype x base-y z sx sy sz r g b a label)
  (hash "id" id "type" ptype
        "x" x "base-y" base-y "y" base-y "z" z
        "sx" sx "sy" sy "sz" sz
        "r" r "g" g "b" b "a" a
        "label" label
        "active" #t
        "timer" 0.0
        "phase" (* (random) 6.28)))

(define (dist-sq x1 z1 x2 z2)
  (let ((dx (- x1 x2))
        (dz (- z1 z2)))
    (+ (* dx dx) (* dz dz))))

(define (take-recent lst n)
  (let loop ((l lst) (k n) (res '()))
    (if (or (null? l) (<= k 0))
        (reverse res)
        (loop (cdr l) (- k 1) (cons (car l) res)))))

(define (player-add-log p msg)
  (let ((cur (hash-try-get p "logs")))
    (let ((lst (if cur cur '())))
      (hash-insert p "logs" (take-recent (cons msg lst) 8)))))

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

  ;; 地牢远古建筑结构
  (set! *entities*
    (list
      (make-entity "dungeon-core" "sphere" 0.0 0.8 0.0 1.8 1.8 1.8 0.75 0.15 0.95 0.95 "Dungeon Core")
      (make-entity "pillar-ne" "cube" 7.0 3.0 7.0 1.6 6.0 1.6 0.35 0.35 0.4 1.0 "Stone Pillar NE")
      (make-entity "pillar-nw" "cube" -7.0 3.0 7.0 1.6 6.0 1.6 0.35 0.35 0.4 1.0 "Stone Pillar NW")
      (make-entity "pillar-se" "cube" 7.0 3.0 -7.0 1.6 6.0 1.6 0.35 0.35 0.4 1.0 "Stone Pillar SE")
      (make-entity "pillar-sw" "cube" -7.0 3.0 -7.0 1.6 6.0 1.6 0.35 0.35 0.4 1.0 "Stone Pillar SW")
      (make-entity "altar-north" "cube" 0.0 1.0 12.0 3.0 1.2 2.0 0.45 0.45 0.5 1.0 "Sacrificial Altar")))

  ;; 地牢散落的生命药剂、敏捷药剂与虚空法球
  (set! *pickups*
    (list
      (make-pickup "pickup-flask-1" "heal" 4.0 0.6 4.0 0.6 0.6 0.6 0.95 0.2 0.3 0.95 "Health Potion (+25 HP)")
      (make-pickup "pickup-flask-2" "heal" -4.0 0.6 -4.0 0.6 0.6 0.6 0.95 0.2 0.3 0.95 "Health Potion (+25 HP)")
      (make-pickup "pickup-elixir-1" "speed" -4.0 0.6 4.0 0.6 0.6 0.6 0.2 0.85 0.95 0.95 "Elixir of Haste (+35%)")
      (make-pickup "pickup-elixir-2" "speed" 4.0 0.6 -4.0 0.6 0.6 0.6 0.2 0.85 0.95 0.95 "Elixir of Haste (+35%)")
      (make-pickup "pickup-mana-1" "score" 0.0 0.6 7.0 0.7 0.7 0.7 0.7 0.3 1.0 0.95 "Mana Crystal (+40 Pts)")
      (make-pickup "pickup-mana-2" "score" 0.0 0.6 -7.0 0.7 0.7 0.7 0.7 0.3 1.0 0.95 "Mana Crystal (+40 Pts)")))

  "Dungeon Mode Initialized")

;; --------------------------------------------------------
;; 2. 冒险者进入地牢
;; --------------------------------------------------------
(define (on-player-join id)
  (let* ((spawn-x (- (* (random) 4.0) 2.0))
         (spawn-z (+ -5.0 (- (* (random) 2.0) 1.0)))
         (pcount (length (hash-keys->list *players*)))
         (color-idx (modulo pcount (length *player-colors*)))
         (pcolor (list-ref *player-colors* color-idx))
         (new-player
           (hash "id" id
                 "x" spawn-x "y" 0.5 "z" spawn-z "yaw" 0.0
                 "hp" 60.0 "max-hp" 60.0 "score" 0 "speed-mult" 1.0
                 "color" pcolor
                 "logs" (list (string-append "Entered Dark Dungeon: " id ". Type 'help' for guidance.")))))
    (set! *players* (hash-insert *players* id new-player))
    (broadcast-log (string-append "Adventurer [" id "] descended into the dungeon.")))
  "joined")

;; --------------------------------------------------------
;; 3. 冒险者离开地牢
;; --------------------------------------------------------
(define (on-player-leave id)
  (set! *players* (hash-remove *players* id))
  (broadcast-log (string-append "Adventurer [" id "] retreated from the dungeon."))
  "left")

;; --------------------------------------------------------
;; 地牢道具拾取判定
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
                         (if (<= dist 2.25)
                             (let* ((ptype (hash-get pk "type"))
                                    (label (hash-get pk "label"))
                                    (updated-player
                                      (cond
                                        ((string=? ptype "heal")
                                         (let* ((cur-hp (hash-get p "hp"))
                                                (max-hp (hash-get p "max-hp"))
                                                (new-hp (min (+ cur-hp 25.0) max-hp))
                                                (new-score (+ (hash-get p "score") 10)))
                                           (player-add-log
                                             (hash-insert (hash-insert p "hp" new-hp) "score" new-score)
                                             (string-append "Consumed " label "! HP restored to " (number->string (round new-hp))))))

                                        ((string=? ptype "speed")
                                         (let* ((cur-mult (hash-get p "speed-mult"))
                                                (new-mult (min (+ cur-mult 0.35) 2.2))
                                                (new-score (+ (hash-get p "score") 20)))
                                           (player-add-log
                                             (hash-insert (hash-insert p "speed-mult" new-mult) "score" new-score)
                                             (string-append "Drank " label "! Speed multiplier: " (number->string new-mult) "x"))))

                                        (else ;; mana crystal
                                         (let ((new-score (+ (hash-get p "score") 40)))
                                           (player-add-log
                                             (hash-insert p "score" new-score)
                                             (string-append "Absorbed " label "! Score +40")))))))
                               (set! *players* (hash-insert *players* id updated-player))
                               (hash-insert (hash-insert pk "active" #f) "timer" 12.0))
                             pk))
                       pk))
                 *pickups*))))))

;; --------------------------------------------------------
;; 4. 冒险者移动
;; --------------------------------------------------------
(define (on-player-move id dx dz yaw)
  (let ((p (hash-try-get *players* id)))
    (if p
        (let* ((mult (hash-try-get p "speed-mult"))
               (speed (* 0.20 (if mult mult 1.0)))
               (new-x (+ (hash-get p "x") (* dx speed)))
               (new-z (+ (hash-get p "z") (* dz speed)))
               (updated (hash-insert (hash-insert (hash-insert p "x" new-x) "z" new-z) "yaw" yaw)))
          (set! *players* (hash-insert *players* id updated))
          (check-player-pickup id)
          "moved")
        "not-found")))

;; --------------------------------------------------------
;; 5. 丰富文本命令集 (help, torch, heal, tp, speed, spawn, say, list, stats)
;; --------------------------------------------------------
(define (on-player-command id cmd)
  (let ((p (hash-try-get *players* id)))
    (if p
        (let* ((tokens (split-whitespace cmd))
               (verb (if (null? tokens) "" (car tokens)))
               (args (if (null? tokens) '() (cdr tokens))))
          (cond
            ;; 5.1 help: 显示命令列表
            ((string=? verb "help")
             (let ((msg "Dungeon commands: help | torch | heal | tp [x z] | speed <factor> | spawn <cube|sphere> <name> | say <text> | list | stats"))
               (set! *players* (hash-insert *players* id (player-add-log p msg)))
               msg))

            ;; 5.2 torch: 点燃并放置一盏地牢壁火把
            ((string=? verb "torch")
             (let* ((torch-id (string-append "torch-" id "-" (number->string (round (* (random) 100000.0)))))
                    (new-torch (make-entity torch-id "sphere"
                                            (hash-get p "x") 1.8 (hash-get p "z")
                                            0.5 0.5 0.5
                                            1.0 0.65 0.15 1.0 "Glowing Torch")))
               (set! *user-entities* (cons new-torch *user-entities*))
               (let* ((msg "System: Placed a blazing dungeon torch!")
                      (updated (player-add-log p msg)))
                 (set! *players* (hash-insert *players* id updated))
                 msg)))

            ;; 5.3 heal: 恢复 100% 生命值
            ((string=? verb "heal")
             (let* ((max-hp (hash-get p "max-hp"))
                    (updated (player-add-log (hash-insert p "hp" max-hp) "System: Health fully restored to maximum.")))
               (set! *players* (hash-insert *players* id updated))
               "System: Health restored to 100%."))

            ;; 5.4 tp [x z]: 传送
            ((string=? verb "tp")
             (cond
               ((null? args)
                (let* ((updated (player-add-log (hash-insert (hash-insert p "x" 0.0) "z" -4.0)
                                                "System: Teleported to entrance (0.0, -4.0).")))
                  (set! *players* (hash-insert *players* id updated))
                  (check-player-pickup id)
                  "System: Teleported to entrance (0.0, -4.0)."))
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
                "System: Usage: tp [x z]")))

            ;; 5.5 speed <factor>: 调整移动速度倍率
            ((string=? verb "speed")
             (if (not (null? args))
                 (let ((factor (string->number (car args))))
                   (if (and factor (> factor 0.0) (<= factor 5.0))
                       (let* ((msg (string-append "System: Dungeon movement speed multiplier set to " (number->string factor) "x."))
                              (updated (player-add-log (hash-insert p "speed-mult" factor) msg)))
                         (set! *players* (hash-insert *players* id updated))
                         msg)
                       "System: Speed multiplier must be between 0.1 and 5.0."))
                 "System: Usage: speed <factor>"))

            ;; 5.6 spawn <shape> <label>: 召唤实体
            ((string=? verb "spawn")
             (if (>= (length args) 2)
                 (let ((shape (car args))
                       (label (string-join (cdr args) " ")))
                   (if (or (string=? shape "cube") (string=? shape "sphere"))
                       (let* ((ent-id (string-append "dungeon-obj-" id "-" (number->string (round (* (random) 100000.0)))))
                              (col (hash-get p "color"))
                              (new-ent (make-entity ent-id shape
                                                    (hash-get p "x") 1.0 (hash-get p "z")
                                                    1.0 1.0 1.0
                                                    (car col) (cadr col) (car (cdr (cdr col))) 0.95
                                                    label)))
                         (set! *user-entities* (cons new-ent *user-entities*))
                         (let* ((msg (string-append "System: Conjured " shape " [" label "] at your location."))
                                (updated (player-add-log p msg)))
                           (set! *players* (hash-insert *players* id updated))
                           msg))
                       "System: Supported shapes are 'cube' or 'sphere'."))
                 "System: Usage: spawn <cube|sphere> <label>"))

            ;; 5.7 say <text>: 聊天广播
            ((string=? verb "say")
             (if (not (null? args))
                 (let* ((text (string-join args " "))
                        (chat (string-append "[Dungeon Chat] " id ": " text)))
                   (broadcast-log chat)
                   (string-append "Broadcasted: " text))
                 "System: Usage: say <message>"))

            ;; 5.8 list: 在线冒险者列表
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
                    (msg (string-append "Dungeon Crawlers (" (number->string (length pids)) "): "
                                        (string-join info-list " | "))))
               (set! *players* (hash-insert *players* id (player-add-log p msg)))
               msg))

            ;; 5.9 stats: 自身属性
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
             (let ((msg (string-append "Unknown command: '" cmd "'. Type 'help' for dungeon commands.")))
               (set! *players* (hash-insert *players* id (player-add-log p msg)))
               msg))))
        "player not found")))

;; --------------------------------------------------------
;; 6. 游戏时钟更新 (Tick) - 地牢核心自转悬浮、火把微光闪烁、药剂重生
;; --------------------------------------------------------
(define (game-tick dt)
  (set! *game-time* (+ *game-time* dt))

  ;; 6.1 地牢核心自转悬浮与暗能呼吸
  (set! *entities*
    (map (lambda (e)
           (if (string=? (hash-get e "id") "dungeon-core")
               (let* ((bob-y (+ 0.8 (* 0.35 (sin (* *game-time* 1.6)))))
                      (pulse-s (+ 1.8 (* 0.15 (cos (* *game-time* 2.4))))))
                 (hash-insert (hash-insert (hash-insert (hash-insert e "y" bob-y)
                                                        "sx" pulse-s)
                                           "sy" pulse-s)
                              "sz" pulse-s))
               e))
         *entities*))

  ;; 6.2 拾取道具能量球动画与复活倒计时
  (set! *pickups*
    (map (lambda (pk)
           (if (hash-get pk "active")
               (let* ((phase (hash-get pk "phase"))
                      (bob-y (+ (hash-get pk "base-y") (* 0.22 (sin (+ (* *game-time* 2.8) phase))))))
                 (hash-insert pk "y" bob-y))
               (let ((rem (- (hash-get pk "timer") dt)))
                 (if (<= rem 0.0)
                     (hash-insert (hash-insert pk "active" #t) "timer" 0.0)
                     (hash-insert pk "timer" rem)))))
         *pickups*))

  ;; 6.3 检查所有玩家与激活道具的碰撞
  (for-each check-player-pickup (hash-keys->list *players*))
  #t)

;; --------------------------------------------------------
;; 7. 个性化客户端状态切片 (地牢迷雾裁剪 + 多人互相可见)
;; --------------------------------------------------------
(define (get-client-state-json id)
  (let ((p (hash-try-get *players* id)))
    (if p
        (let* ((px (hash-get p "x"))
               (pz (hash-get p "z"))
               (max-dist-sq (* *view-distance* *view-distance*))

               ;; (1) 视野内的静态地牢实体
               (visible-scene-ents
                 (filter (lambda (e)
                           (<= (dist-sq px pz (hash-get e "x") (hash-get e "z")) max-dist-sq))
                         *entities*))

               ;; (2) 视野内的冒险者放置火把/实体
               (visible-user-ents
                 (filter (lambda (e)
                           (<= (dist-sq px pz (hash-get e "x") (hash-get e "z")) max-dist-sq))
                         *user-entities*))

               ;; (3) 视野内的激活拾取道具
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

               ;; (4) 地牢中其他可见冒险者 (shape=cube, label=ID, 专属光芒颜色)
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

               ;; 汇总全部可见图元
               (all-visible-ents
                 (append visible-scene-ents
                         visible-user-ents
                         visible-pickup-ents
                         visible-other-players))

               ;; 转换为 JSON 图元格式
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

               ;; 玩家地牢探索日志
               (player-logs (hash-try-get p "logs"))
               (logs-json
                 (if (and player-logs (not (null? player-logs)))
                     (string-join
                       (map (lambda (m)
                              (let ((safe-m (string-join (split-whitespace m) " ")))
                                (string-append "\"" safe-m "\"")))
                            player-logs)
                       ",")
                     "\"In Dungeon\"")))

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
