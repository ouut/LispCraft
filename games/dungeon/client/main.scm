;; ========================================================
;; games/dungeon/client/main.scm - Dungeon Client Rendering
;; ========================================================

(define *dungeon-client-time* 0.0)

(define (client-init)
  (displayln "[ClientScheme] Dungeon client initialized.")
  (set! *dungeon-client-time* 0.0))

(define (draw-dungeon-entity ent)
  (let ((id (cdr (assoc "id" ent)))
        (shape (cdr (assoc "shape" ent)))
        (pos (cdr (assoc "pos" ent)))
        (size (cdr (assoc "size" ent)))
        (color (cdr (assoc "color" ent)))
        (label (cdr (assoc "label" ent))))
    (cond
      ;; 玩家
      ((string-starts-with? id "player-")
       (let ((px (list-ref pos 0))
             (py (list-ref pos 1))
             (pz (list-ref pos 2)))
         (draw-cube! (list px (+ py 0.6) pz) '(0.6 0.8 0.6) color)
         (draw-cube-wires! (list px (+ py 0.6) pz) '(0.62 0.82 0.62) '(1.0 0.8 0.2 0.8))
         (draw-sphere! (list px (+ py 1.2) pz) 0.25 '(0.9 0.8 0.7 1.0))))
      ;; 立方体图元 (石柱/祭坛)
      ((string=? shape "cube")
       (draw-cube! pos size color)
       (draw-cube-wires! pos size '(0.4 0.4 0.5 0.6)))
      ;; 球体图元 (核心/药水)
      ((string=? shape "sphere")
       (let ((r (* (list-ref size 0) 0.5)))
         (draw-sphere! pos r color)
         (draw-sphere-wires! pos r '(1.0 0.6 0.2 0.5))))
      (else
       (draw-cube! pos size color)))

    ;; 浮动文本
    (if (and label (> (string-length label) 0))
        (let ((label-y (+ (list-ref pos 1) (list-ref size 1) 0.4)))
          (draw-billboard-text! label (list (list-ref pos 0) label-y (list-ref pos 2)) 14.0 '(1.0 0.9 0.7 0.95)))
        #f)))

(define (client-render dt)
  (set! *dungeon-client-time* (+ *dungeon-client-time* dt))

  ;; 1. 地牢石板地面 (Dark Stone Floor)
  (draw-plane! '(0.0 0.0 0.0) '(80.0 80.0) '(0.06 0.06 0.08 1.0))
  (let loop ((x -30.0))
    (if (<= x 30.0)
        (begin
          (draw-line-3d! (list x 0.02 -30.0) (list x 0.02 30.0) '(0.15 0.15 0.2 0.3))
          (draw-line-3d! (list -30.0 0.02 x) (list 30.0 0.02 x) '(0.15 0.15 0.2 0.3))
          (loop (+ x 3.0)))
        #f))

  ;; 2. 遍历渲染实体
  (let ((entities (get-replicated-entities)))
    (if (list? entities)
        (for-each draw-dungeon-entity entities)
        #f))

  ;; 3. 本地主角
  (let ((me (get-local-player)))
    (if (and (list? me) (assoc "pos" me))
        (let ((pos (cdr (assoc "pos" me))))
          (let ((px (list-ref pos 0))
                (py (list-ref pos 1))
                (pz (list-ref pos 2)))
            (draw-cube! (list px (+ py 0.6) pz) '(0.6 0.8 0.6) '(0.85 0.35 0.15 1.0))
            (draw-sphere! (list px (+ py 1.2) pz) 0.25 '(1.0 0.8 0.6 1.0))
            ;; 主角周身微弱火把光晕
            (draw-plane! (list px (+ py 0.02) pz) '(2.5 2.5) '(1.0 0.6 0.2 0.15))))
        #f)

    ;; 4. 简易地牢 HUD
    (let ((sw (screen-width))
          (sh (screen-height)))
      (draw-rect! 15.0 12.0 260.0 36.0 '(0.08 0.05 0.05 0.8))
      (draw-rect-lines! 15.0 12.0 260.0 36.0 1.5 '(0.8 0.4 0.2 0.8))
      (draw-text-screen! "DUNGEON EXPLORER [DARK REALM]" 25.0 35.0 15.0 '(1.0 0.6 0.2 1.0))
      (if (list? me)
          (let ((hp (cdr (assoc "hp" me)))
                (score (cdr (assoc "score" me))))
            (draw-text-screen! (string-append "HP: " (to-string (round hp))) 15.0 65.0 14.0 '(1.0 0.3 0.3 1.0))
            (draw-text-screen! (string-append "LOOT: " (to-string score)) 100.0 65.0 14.0 '(1.0 0.8 0.2 1.0)))
          #f))))

(define (string-starts-with? s prefix)
  (let ((s-len (string-length s))
        (p-len (string-length prefix)))
    (if (< s-len p-len)
        #f
        (string=? (substring s 0 p-len) prefix))))

(define (to-string val)
  (cond
    ((number? val) (number->string val))
    ((string? val) val)
    (else "")))
