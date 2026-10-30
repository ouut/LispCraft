;; ========================================================
;; games/space_arena/client/main.scm - Client Main Rendering Entry
;; ========================================================

;; 全局客户端累积时钟
(define *client-time* 0.0)

;; 客户端初始化回调
(define (client-init)
  (displayln "[ClientScheme] Space Arena client initialized.")
  (set! *client-time* 0.0))

;; 绘制单个远端实体
(define (draw-replicated-entity ent)
  (let ((id (cdr (assoc "id" ent)))
        (shape (cdr (assoc "shape" ent)))
        (pos (cdr (assoc "pos" ent)))
        (size (cdr (assoc "size" ent)))
        (color (cdr (assoc "color" ent)))
        (label (cdr (assoc "label" ent))))
    (cond
      ;; 其他玩家使用机甲绘制
      ((string-starts-with? id "player-")
       (draw-player-avatar pos 0.0 color #f))
      ;; 立方体图元
      ((string=? shape "cube")
       (draw-cube! pos size color)
       (draw-cube-wires! pos size '(0.9 0.95 1.0 0.4)))
      ;; 球体图元
      ((string=? shape "sphere")
       (let ((r (* (list-ref size 0) 0.5)))
         (draw-sphere! pos r color)
         (draw-sphere-wires! pos r '(0.9 0.95 1.0 0.4))))
      ;; 默认兜底
      (else
       (draw-cube! pos size color)))

    ;; 浮动头顶 3D 文字标牌
    (if (and label (> (string-length label) 0))
        (let ((label-y (+ (list-ref pos 1) (list-ref size 1) 0.5)))
          (draw-billboard-text! label (list (list-ref pos 0) label-y (list-ref pos 2)) 14.0 '(1.0 1.0 1.0 0.95)))
        #f)))

;; 客户端主渲染循环 (由 Rust Macroquad 宿主每帧调用)
(define (client-render dt)
  (set! *client-time* (+ *client-time* dt))

  ;; 1. 3D 背景网格 (Cyber Grid)
  (draw-plane! '(0.0 0.0 0.0) '(100.0 100.0) '(0.02 0.03 0.06 1.0))
  (let loop ((x -40.0))
    (if (<= x 40.0)
        (begin
          (draw-line-3d! (list x 0.02 -40.0) (list x 0.02 40.0) '(0.1 0.18 0.3 0.4))
          (draw-line-3d! (list -40.0 0.02 x) (list 40.0 0.02 x) '(0.1 0.18 0.3 0.4))
          (loop (+ x 2.0)))
        #f))

  ;; 2. 遍历渲染服务端属性复制的所有实体
  (let ((entities (get-replicated-entities)))
    (if (list? entities)
        (for-each draw-replicated-entity entities)
        #f))

  ;; 3. 渲染本地主角机甲
  (let ((me (get-local-player)))
    (if (and (list? me) (assoc "pos" me))
        (let ((pos (cdr (assoc "pos" me)))
              (yaw (cdr (assoc "yaw" me))))
          (draw-player-avatar pos yaw '(0.2 0.85 1.0 1.0) #t))
        #f)

    ;; 4. 绘制 2D HUD 仪表盘与小地图
    (let ((sw (screen-width))
          (sh (screen-height)))
      (if (list? me)
          (draw-game-hud me sw sh)
          #f))))

;; 字符串前缀辅助函数
(define (string-starts-with? s prefix)
  (let ((s-len (string-length s))
        (p-len (string-length prefix)))
    (if (< s-len p-len)
        #f
        (string=? (substring s 0 p-len) prefix))))

;; 辅助转字符串
(define (to-string val)
  (cond
    ((number? val) (number->string val))
    ((string? val) val)
    (else "")))
