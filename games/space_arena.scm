;; ========================================================
;; games/space_arena.scm - LispCraft 多人 3D 游戏逻辑
;; ========================================================

;; 玩家表: id -> hash-map
(define *players* (hash))

;; 场景实体列表
(define *entities* '())

;; 辅助函数：创建实体
(define (make-entity id shape x y z sx sy sz r g b a label)
  (hash "id" id "shape" shape "x" x "y" y "z" z
        "sx" sx "sy" sy "sz" sz
        "r" r "g" g "b" b "a" a
        "label" label))

;; 1. 游戏初始化
(define (init-game)
  (set! *players* (hash))
  (set! *entities*
    (list
      (make-entity "beacon-center" "sphere" 0.0 1.0 0.0 1.2 1.2 1.2 0.2 0.8 1.0 0.9 "Central Beacon")
      (make-entity "monolith-1" "cube" 10.0 2.0 10.0 2.0 4.0 2.0 0.9 0.2 0.2 1.0 "Red Monolith")
      (make-entity "monolith-2" "cube" -10.0 2.0 -10.0 2.0 4.0 2.0 0.2 0.9 0.2 1.0 "Green Monolith")
      (make-entity "monolith-3" "cube" 10.0 2.0 -10.0 2.0 4.0 2.0 0.9 0.8 0.2 1.0 "Gold Monolith")))
  "Space Arena Initialized")

;; 2. 玩家加入
(define (on-player-join id)
  (let ((spawn-x (- (* (random) 10.0) 5.0))
        (spawn-z (- (* (random) 10.0) 5.0)))
    (set! *players*
          (hash-insert *players* id
            (hash "x" spawn-x "y" 0.5 "z" spawn-z "yaw" 0.0
                  "hp" 100.0 "max-hp" 100.0 "score" 0
                  "logs" (list (string-append "Welcome Commander " id "!"))))))
  "joined")

;; 3. 玩家断开
(define (on-player-leave id)
  (set! *players* (hash-remove *players* id))
  "left")

;; 4. 玩家移动 (根据键盘输入及视角偏航计算新位置)
(define (on-player-move id dx dz yaw)
  (let ((p (hash-get *players* id)))
    (if p
        (let* ((speed 0.25)
               (new-x (+ (hash-get p "x") (* dx speed)))
               (new-z (+ (hash-get p "z") (* dz speed)))
               (updated (hash-insert (hash-insert (hash-insert p "x" new-x) "z" new-z) "yaw" yaw)))
          (set! *players* (hash-insert *players* id updated))
          "moved")
        "not-found")))

;; 5. 玩家字符串命令解析
(define (on-player-command id cmd)
  (let ((p (hash-get *players* id)))
    (if p
        (cond
          ;; heal: 恢复生命值
          ((string=? cmd "heal")
           (let ((updated (hash-insert p "hp" 100.0)))
             (set! *players* (hash-insert *players* id updated))
             "System: Health restored to 100%."))

          ;; tp: 传送回原点
          ((string=? cmd "tp")
           (let ((updated (hash-insert (hash-insert p "x" 0.0) "z" 0.0)))
             (set! *players* (hash-insert *players* id updated))
             "System: Teleported to central beacon (0, 0)."))

          ;; spawn <label>: 生成一个发光方块
          ((and (> (string-length cmd) 6) (string=? (substring cmd 0 6) "spawn "))
           (let* ((label (substring cmd 6 (string-length cmd)))
                  (new-ent (make-entity (string-append "ent-" id "-" (number->string (random))) "cube"
                                        (hash-get p "x") 1.0 (hash-get p "z")
                                        1.5 1.5 1.5 1.0 0.6 0.1 1.0 label)))
             (set! *entities* (cons new-ent *entities*))
             (string-append "System: Spawned entity [" label "] at your location.")))

          (else
           (string-append "Unknown command: '" cmd "'. Try: heal, tp, spawn <name>")))
        "player not found")))

;; 6. 游戏时钟 Tick
(define (game-tick dt)
  ;; 世界演进逻辑（如自转、定时生成等）
  #t)

;; 7. 个性化客户端状态切片 (下发 JSON)
(define (get-client-state-json id)
  (let ((p (hash-get *players* id)))
    (if p
        (string-append
          "{"
          "\"client_id\":\"" id "\","
          "\"camera_target\":[" (number->string (hash-get p "x")) "," (number->string (hash-get p "y")) "," (number->string (hash-get p "z")) "],"
          "\"camera_yaw\":" (number->string (hash-get p "yaw")) ","
          "\"hp\":" (number->string (hash-get p "hp")) ","
          "\"max_hp\":" (number->string (hash-get p "max-hp")) ","
          "\"score\":" (number->string (hash-get p "score")) ","
          "\"entities\":["
            (string-join
              (map (lambda (e)
                     (string-append
                       "{\"id\":\"" (hash-get e "id") "\","
                       "\"shape\":\"" (hash-get e "shape") "\","
                       "\"pos\":[" (number->string (hash-get e "x")) "," (number->string (hash-get e "y")) "," (number->string (hash-get e "z")) "],"
                       "\"size\":[" (number->string (hash-get e "sx")) "," (number->string (hash-get e "sy")) "," (number->string (hash-get e "sz")) "],"
                       "\"color\":[" (number->string (hash-get e "r")) "," (number->string (hash-get e "g")) "," (number->string (hash-get e "b")) "," (number->string (hash-get e "a")) "],"
                       "\"label\":\"" (hash-get e "label") "\"}"))
                   *entities*)
              ",")
          "],"
          "\"logs\":[\"Online\"]"
          "}")
        "{}")))
