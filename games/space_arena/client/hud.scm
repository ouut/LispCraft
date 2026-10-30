;; ========================================================
;; games/space_arena/client/hud.scm - 2D HUD & Radar Rendering
;; ========================================================

(define (draw-game-hud player-data sw sh)
  (let ((hp (cdr (assoc "hp" player-data)))
        (max-hp (cdr (assoc "max_hp" player-data)))
        (score (cdr (assoc "score" player-data)))
        (pos (cdr (assoc "pos" player-data)))
        (yaw (cdr (assoc "yaw" player-data))))

    ;; 1. 顶部标题面板
    (draw-rect! 15.0 12.0 280.0 38.0 '(0.05 0.08 0.15 0.75))
    (draw-rect-lines! 15.0 12.0 280.0 38.0 1.5 '(0.2 0.8 1.0 0.8))
    (draw-text-screen! "LISPCRAFT 3D [SCHEME DRIVEN]" 25.0 36.0 16.0 '(0.0 1.0 0.85 1.0))

    ;; 2. 生命值槽 (HP Bar)
    (let* ((bar-w 220.0)
           (bar-h 16.0)
           (fill-w (* bar-w (/ (max 0.0 hp) (max 1.0 max-hp)))))
      (draw-rect! 15.0 58.0 bar-w bar-h '(0.1 0.12 0.18 0.8))
      (draw-rect! 15.0 58.0 fill-w bar-h '(0.2 0.9 0.4 0.9))
      (draw-rect-lines! 15.0 58.0 bar-w bar-h 1.0 '(0.4 0.6 0.8 0.8))
      (draw-text-screen! (string-append "HP: " (to-string (round hp))) 245.0 71.0 14.0 '(1.0 1.0 1.0 0.9)))

    ;; 3. 积分与坐标指示
    (draw-text-screen! (string-append "SCORE: " (to-string score)) 15.0 95.0 15.0 '(1.0 0.85 0.2 1.0))
    (if (and pos (= (length pos) 3))
        (let ((cx (round (list-ref pos 0)))
              (cz (round (list-ref pos 2))))
          (draw-text-screen! (string-append "POS: " (to-string cx) ", " (to-string cz)) 15.0 115.0 13.0 '(0.7 0.8 0.9 0.75))
          #f)
        #f)

    ;; 4. 左下角战术小地图雷达 (2D Radar)
    (let ((rx 90.0)
          (ry (- sh 90.0))
          (radius 65.0))
      (draw-circle! rx ry radius '(0.03 0.06 0.12 0.75))
      (draw-circle-lines! rx ry radius 2.0 '(0.2 0.8 1.0 0.85))
      (draw-circle-lines! rx ry (* radius 0.6) 1.0 '(0.2 0.5 0.8 0.4))
      (draw-circle-lines! rx ry (* radius 0.3) 1.0 '(0.2 0.5 0.8 0.4))
      ;; 中心主角点
      (draw-circle! rx ry 3.5 '(0.0 1.0 0.8 1.0))
      ;; 十字分划
      (draw-line-2d! (- rx radius) ry (+ rx radius) ry 1.0 '(0.2 0.5 0.8 0.3))
      (draw-line-2d! rx (- ry radius) rx (+ ry radius) 1.0 '(0.2 0.5 0.8 0.3))
      (draw-text-screen! "N" (- rx 4.0) (- ry radius -12.0) 11.0 '(0.2 0.8 1.0 0.8)))))
