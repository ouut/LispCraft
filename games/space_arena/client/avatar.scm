;; ========================================================
;; games/space_arena/client/avatar.scm - 3D Avatar Rendering
;; ========================================================

;; 绘制单个 3D 机甲化身
(define (draw-player-avatar pos yaw color is-local)
  (let ((px (list-ref pos 0))
        (py (list-ref pos 1))
        (pz (list-ref pos 2)))
    ;; 1. 身体躯干 (Torso)
    (draw-cube! (list px (+ py 0.6) pz) '(0.7 0.9 0.5) color)
    (draw-cube-wires! (list px (+ py 0.6) pz) '(0.72 0.92 0.52) '(0.9 0.95 1.0 0.8))

    ;; 2. 核心反应堆 (Reactor Core)
    (draw-sphere! (list px (+ py 0.65) (+ pz 0.28)) 0.18 '(0.2 0.9 1.0 0.9))

    ;; 3. 头部视镜 (Visor/Helmet)
    (draw-cube! (list px (+ py 1.25) pz) '(0.45 0.35 0.45) '(0.15 0.2 0.3 1.0))
    (draw-cube! (list px (+ py 1.25) (+ pz 0.18)) '(0.35 0.12 0.1) '(0.0 0.9 1.0 1.0))

    ;; 4. 悬浮推进器尾焰 (Thruster Flame)
    (if is-local
        (draw-cylinder! (list px (+ py 0.1) (- pz 0.2)) 0.15 0.02 0.45 6 '(0.2 0.6 1.0 0.7))
        (draw-cylinder! (list px (+ py 0.1) (- pz 0.2)) 0.12 0.02 0.35 6 '(1.0 0.4 0.1 0.7)))

    ;; 5. 脚底光环 (Aura Ring)
    (draw-plane! (list px (+ py 0.02) pz) '(1.4 1.4) (if is-local '(0.2 0.8 1.0 0.4) '(1.0 0.3 0.3 0.3)))))
