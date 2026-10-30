;; ========================================================
;; games/tank_battle_2d/client/main.scm
;; Cyber Tank Battle 2D - Pure 2D Retro Arcade Client Renderer
;; ========================================================

;; 客户端累积动画与特效时钟
(define *client-time* 0.0)

;; 客户端初始化生命周期回调 (启动与热重载时触发)
(define (client-init)
  (displayln "[ClientScheme] Cyber Tank Battle 2D renderer initialized.")
  (set! *client-time* 0.0))

;; --------------------------------------------------------
;; 1. 基础投影与数学工具
;; --------------------------------------------------------

;; 两点距离平方
(define (dist-sq x1 z1 x2 z2)
  (let ((dx (- x1 x2))
        (dz (- z1 z2)))
    (+ (* dx dx) (* dz dz))))

;; 字符串前缀检查
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

;; 世界坐标 (wx, wz) 投影为 2D 屏幕坐标 (sx, sy)
(define (world->screen wx wz px pz cx cy scale)
  (let ((dx (- wx px))
        (dz (- wz pz)))
    (list (+ cx (* dx scale))
          (+ cy (* dz scale)))))

;; --------------------------------------------------------
;; 2. 2D 街机风格网格背景与边界屏障渲染
;; --------------------------------------------------------
(define (draw-arena-grid sw sh px pz cx cy scale)
  ;; 2.1 纯黑深空背景底色
  (draw-rect! 0.0 0.0 sw sh '(0.02 0.03 0.06 1.0))

  ;; 2.2 随玩家位置平滑滚动的 2D 赛博网格线
  (let* ((grid-step (* 4.0 scale)) ;; 每 4 米一条主网格线
         (offset-x (modulo (- (* px scale)) grid-step))
         (offset-y (modulo (- (* pz scale)) grid-step)))

    ;; 垂直网格线
    (let loop-x ((gx (- offset-x grid-step)))
      (if (<= gx (+ sw grid-step))
          (begin
            (draw-line-2d! gx 0.0 gx sh 1.0 '(0.08 0.14 0.24 0.45))
            (loop-x (+ gx grid-step)))
          #f))

    ;; 水平网格线
    (let loop-y ((gy (- offset-y grid-step)))
      (if (<= gy (+ sh grid-step))
          (begin
            (draw-line-2d! 0.0 gy sw gy 1.0 '(0.08 0.14 0.24 0.45))
            (loop-y (+ gy grid-step)))
          #f)))

  ;; 2.3 竞技场外围警示能量墙 (±24.0 米边界)
  (let* ((bound-min -24.0)
         (bound-max  24.0)
         (p1 (world->screen bound-min bound-min px pz cx cy scale))
         (p2 (world->screen bound-max bound-min px pz cx cy scale))
         (p3 (world->screen bound-max bound-max px pz cx cy scale))
         (p4 (world->screen bound-min bound-max px pz cx cy scale))
         (x1 (list-ref p1 0)) (y1 (list-ref p1 1))
         (x2 (list-ref p2 0)) (y2 (list-ref p2 1))
         (x3 (list-ref p3 0)) (y3 (list-ref p3 1))
         (x4 (list-ref p4 0)) (y4 (list-ref p4 1)))
    ;; 四条外围高能脉冲屏障
    (draw-line-2d! x1 y1 x2 y2 3.0 '(1.0 0.45 0.1 0.85))
    (draw-line-2d! x2 y2 x3 y3 3.0 '(1.0 0.45 0.1 0.85))
    (draw-line-2d! x3 y3 x4 y4 3.0 '(1.0 0.45 0.1 0.85))
    (draw-line-2d! x4 y4 x1 y1 3.0 '(1.0 0.45 0.1 0.85))
    ;; 四个角落防御哨塔
    (draw-circle! x1 y1 8.0 '(1.0 0.45 0.1 0.9))
    (draw-circle! x2 y2 8.0 '(1.0 0.45 0.1 0.9))
    (draw-circle! x3 y3 8.0 '(1.0 0.45 0.1 0.9))
    (draw-circle! x4 y4 8.0 '(1.0 0.45 0.1 0.9))))

;; --------------------------------------------------------
;; 3. 2D 战车全套几何图元渲染 (机体、双履带、炮台、炮管、尾焰、护盾)
;; --------------------------------------------------------
(define (draw-tank-sprite sx sy yaw color is-local hp max-hp label)
  (let* (;; 航向向量与法向向量
         (dir-x (sin yaw))
         (dir-y (* -1.0 (cos yaw)))
         (perp-x (cos yaw))
         (perp-y (sin yaw))

         ;; 履带中心偏移
         (tread-dist 14.0)
         (lx (- sx (* perp-x tread-dist)))
         (ly (- sy (* perp-y tread-dist)))
         (rx (+ sx (* perp-x tread-dist)))
         (ry (+ sy (* perp-y tread-dist)))

         ;; 战车底盘外形
         (hull-w 22.0)
         (hull-h 30.0))

    ;; 3.1 履带 (左履带与右履带)
    (draw-rect! (- lx 4.0) (- ly 14.0) 8.0 28.0 '(0.12 0.15 0.2 1.0))
    (draw-rect-lines! (- lx 4.0) (- ly 14.0) 8.0 28.0 1.5 '(0.3 0.4 0.5 0.85))
    (draw-line-2d! (- lx 4.0) (- ly 7.0) (+ lx 4.0) (- ly 7.0) 1.0 '(0.4 0.5 0.6 0.8))
    (draw-line-2d! (- lx 4.0) ly (+ lx 4.0) ly 1.0 '(0.4 0.5 0.6 0.8))
    (draw-line-2d! (- lx 4.0) (+ ly 7.0) (+ lx 4.0) (+ ly 7.0) 1.0 '(0.4 0.5 0.6 0.8))

    (draw-rect! (- rx 4.0) (- ry 14.0) 8.0 28.0 '(0.12 0.15 0.2 1.0))
    (draw-rect-lines! (- rx 4.0) (- ry 14.0) 8.0 28.0 1.5 '(0.3 0.4 0.5 0.85))
    (draw-line-2d! (- rx 4.0) (- ry 7.0) (+ rx 4.0) (- ry 7.0) 1.0 '(0.4 0.5 0.6 0.8))
    (draw-line-2d! (- rx 4.0) ry (+ rx 4.0) ry 1.0 '(0.4 0.5 0.6 0.8))
    (draw-line-2d! (- rx 4.0) (+ ry 7.0) (+ rx 4.0) (+ ry 7.0) 1.0 '(0.4 0.5 0.6 0.8))

    ;; 3.2 战车装甲主体机舱
    (let ((cr (car color))
          (cg (cadr color))
          (cb (car (cdr (cdr color)))))
      (draw-rect! (- sx 10.0) (- sy 13.0) 20.0 26.0 (list cr cg cb 0.88))
      (draw-rect-lines! (- sx 10.0) (- sy 13.0) 20.0 26.0 2.0 '(0.95 1.0 1.0 0.95)))

    ;; 3.3 旋转主炮台底座
    (draw-circle! sx sy 9.0 '(0.1 0.14 0.2 1.0))
    (draw-circle-lines! sx sy 9.0 1.5 color)

    ;; 3.4 主炮炮管与制退器 (沿 yaw 朝向延伸 22 像素)
    (let* ((barrel-len 22.0)
           (barrel-end-x (+ sx (* dir-x barrel-len)))
           (barrel-end-y (+ sy (* dir-y barrel-len))))
      ;; 炮管外壁与高亮内芯
      (draw-line-2d! sx sy barrel-end-x barrel-end-y 5.0 '(0.25 0.3 0.4 1.0))
      (draw-line-2d! sx sy barrel-end-x barrel-end-y 2.2 '(0.8 0.95 1.0 0.9))
      ;; 炮口制退器环
      (draw-circle! barrel-end-x barrel-end-y 3.2 '(1.0 0.95 0.4 1.0)))

    ;; 3.5 炮塔中央反应堆核心 (呼吸灯特效)
    (draw-circle! sx sy 4.2 '(0.2 0.85 1.0 1.0))
    (draw-circle! sx sy 2.0 '(1.0 1.0 1.0 1.0))

    ;; 3.6 引擎喷气尾焰 (位于战车后方，高频脉动)
    (let* ((rear-dist 15.0)
           (rear-x (- sx (* dir-x rear-dist)))
           (rear-y (- sy (* dir-y rear-dist)))
           (flame-len (+ 7.0 (* 4.0 (sin (* *client-time* 24.0)))))
           (flame-tip-x (- rear-x (* dir-x flame-len)))
           (flame-tip-y (- rear-y (* dir-y flame-len))))
      (draw-line-2d! rear-x rear-y flame-tip-x flame-tip-y 4.0 '(1.0 0.45 0.1 0.85))
      (draw-line-2d! rear-x rear-y flame-tip-x flame-tip-y 1.8 '(1.0 0.95 0.3 0.95)))

    ;; 3.7 能量偏折护盾环 (当生命值健康或护盾激活时呈现)
    (if (> hp 40.0)
        (draw-circle-lines! sx sy 24.0 1.5 '(0.2 0.8 1.0 0.55))
        #f)

    ;; 3.8 头顶 2D 迷你血条与呼号标签
    (let* ((bar-w 36.0)
           (bar-h 4.0)
           (bar-x (- sx 18.0))
           (bar-y (- sy 26.0))
           (hp-ratio (/ (max 0.0 hp) (max 1.0 max-hp)))
           (fill-w (* bar-w hp-ratio))
           (hp-col (cond
                     ((> hp-ratio 0.5) '(0.2 0.9 0.4 0.95))
                     ((> hp-ratio 0.25) '(1.0 0.8 0.2 0.95))
                     (else '(1.0 0.25 0.25 0.95)))))
      (draw-rect! bar-x bar-y bar-w bar-h '(0.08 0.1 0.14 0.8))
      (draw-rect! bar-x bar-y fill-w bar-h hp-col)
      (draw-rect-lines! bar-x bar-y bar-w bar-h 1.0 '(0.3 0.5 0.7 0.8))
      (if (and label (> (string-length label) 0))
          (let ((tw (measure-text-width label 11.0)))
            (draw-text-screen! label (- sx (/ tw 2.0)) (- bar-y 3.0) 11.0 '(0.9 0.95 1.0 0.9)))
          #f))))

;; --------------------------------------------------------
;; 4. 敌方战术无人机与巡逻重坦 2D 绘制
;; --------------------------------------------------------
(define (draw-enemy-drone ent px pz cx cy scale)
  (let* ((pos (cdr (assoc "pos" ent)))
         (shape (cdr (assoc "shape" ent)))
         (label (cdr (assoc "label" ent)))
         (wx (list-ref pos 0))
         (wz (list-ref pos 2))
         (s-pos (world->screen wx wz px pz cx cy scale))
         (sx (list-ref s-pos 0))
         (sy (list-ref s-pos 1)))

    (cond
      ;; 4.1 侦察无人机 (Scout Drone) - 敏捷六角飞碟造型 + 巡视红光
      ((string-starts-with? shape "drone-scout")
       (draw-circle! sx sy 12.0 '(0.25 0.08 0.1 0.9))
       (draw-circle-lines! sx sy 12.0 2.0 '(1.0 0.25 0.25 0.95))
       (draw-circle! sx sy 4.5 '(1.0 0.15 0.15 1.0))
       (draw-circle! sx sy 1.8 '(1.0 0.85 0.85 1.0))
       ;; 旋转探测信标天线
       (let* ((angle (* *client-time* 4.0))
              (ax (+ sx (* (cos angle) 16.0)))
              (ay (+ sy (* (sin angle) 16.0))))
         (draw-line-2d! sx sy ax ay 1.5 '(1.0 0.3 0.3 0.75))))

      ;; 4.2 突击无人重坦 (Assault Drone) - 坚固正方棱角 + 猩红核心
      ((string-starts-with? shape "drone-assault")
       (draw-rect! (- sx 12.0) (- sy 12.0) 24.0 24.0 '(0.3 0.12 0.08 0.9))
       (draw-rect-lines! (- sx 12.0) (- sy 12.0) 24.0 24.0 2.0 '(1.0 0.4 0.1 0.95))
       (draw-circle! sx sy 6.0 '(1.0 0.3 0.1 1.0))
       ;; 双联突击炮管
       (draw-line-2d! (- sx 5.0) sy (- sx 5.0) (+ sy 16.0) 3.0 '(0.3 0.3 0.4 1.0))
       (draw-line-2d! (+ sx 5.0) sy (+ sx 5.0) (+ sy 16.0) 3.0 '(0.3 0.3 0.4 1.0)))

      ;; 4.3 泰坦巨兽战车 (Boss Titan) - 多重防护力场与巨型核反应堆
      ((string-starts-with? shape "drone-boss")
       (draw-circle! sx sy 24.0 '(0.2 0.04 0.08 0.92))
       (draw-circle-lines! sx sy 24.0 3.0 '(1.0 0.15 0.25 1.0))
       (draw-circle-lines! sx sy 16.0 1.5 '(1.0 0.5 0.1 0.8))
       (draw-circle! sx sy 8.0 '(1.0 0.2 0.2 1.0))
       (draw-circle! sx sy 4.0 '(1.0 0.9 0.9 1.0)))

      ;; 兜底敌机图元
      (else
       (draw-circle! sx sy 11.0 '(0.8 0.2 0.2 0.9))
       (draw-circle-lines! sx sy 11.0 1.5 '(1.0 0.4 0.4 0.9))))

    ;; 敌机头顶名字与血量标牌
    (if (and label (> (string-length label) 0))
        (let ((tw (measure-text-width label 11.0)))
          (draw-text-screen! label (- sx (/ tw 2.0)) (- sy 20.0) 11.0 '(1.0 0.45 0.45 0.95)))
        #f)))

;; --------------------------------------------------------
;; 5. 战场战术道具球 2D 悬浮动画渲染
;; --------------------------------------------------------
(define (draw-pickup-item ent px pz cx cy scale)
  (let* ((id (cdr (assoc "id" ent)))
         (pos (cdr (assoc "pos" ent)))
         (label (cdr (assoc "label" ent)))
         (wx (list-ref pos 0))
         (wz (list-ref pos 2))
         (s-pos (world->screen wx wz px pz cx cy scale))
         (sx (list-ref s-pos 0))
         (sy (list-ref s-pos 1))
         ;; 动态呼吸灯脉动半径
         (pulse-r (+ 9.0 (* 2.2 (sin (* *client-time* 5.0))))))

    (cond
      ;; 5.1 纳米装甲修复球 (Repair Kit) - 翡翠绿底座 + 悬浮白十字 + 扩散光环
      ((string-starts-with? id "pickup-repair")
       (draw-circle! sx sy pulse-r '(0.1 0.85 0.35 0.8))
       (draw-circle-lines! sx sy (+ pulse-r 4.0) 1.5 '(0.4 1.0 0.6 0.6))
       ;; 白十字标志
       (draw-line-2d! (- sx 4.5) sy (+ sx 4.5) sy 2.5 '(1.0 1.0 1.0 1.0))
       (draw-line-2d! sx (- sy 4.5) sx (+ sy 4.5) 2.5 '(1.0 1.0 1.0 1.0)))

      ;; 5.2 电浆过载充能球 (Overcharge) - 辉煌金黄 + 四芒星脉冲
      ((string-starts-with? id "pickup-overcharge")
       (draw-circle! sx sy pulse-r '(1.0 0.75 0.1 0.85))
       (draw-circle-lines! sx sy (+ pulse-r 4.0) 1.5 '(1.0 0.9 0.4 0.6))
       ;; 四芒能量星
       (draw-line-2d! (- sx 6.0) sy (+ sx 6.0) sy 2.0 '(1.0 1.0 0.9 1.0))
       (draw-line-2d! sx (- sy 6.0) sx (+ sy 6.0) 2.0 '(1.0 1.0 0.9 1.0)))

      ;; 5.3 偏折力场护盾电池 (Shield Battery) - 电浆青蓝同心能量环
      ((string-starts-with? id "pickup-shield")
       (draw-circle! sx sy pulse-r '(0.15 0.75 1.0 0.85))
       (draw-circle-lines! sx sy (+ pulse-r 4.0) 2.0 '(0.6 0.9 1.0 0.7))
       (draw-circle-lines! sx sy (+ pulse-r 7.0) 1.0 '(0.6 0.9 1.0 0.4)))

      ;; 5.4 EMP 战术核心 (EMP Core) - 灵能紫环与震荡波
      (else
       (draw-circle! sx sy pulse-r '(0.75 0.25 1.0 0.85))
       (draw-circle-lines! sx sy (+ pulse-r 5.0) 1.5 '(0.9 0.5 1.0 0.65))))

    ;; 浮动提示文本
    (if (and label (> (string-length label) 0))
        (let ((tw (measure-text-width label 10.0)))
          (draw-text-screen! label (- sx (/ tw 2.0)) (- sy (+ pulse-r 6.0)) 10.0 '(0.9 0.95 1.0 0.85)))
        #f)))

;; --------------------------------------------------------
;; 6. 战场掩体堡垒与电浆炮弹渲染
;; --------------------------------------------------------
(define (draw-obstacle-bunker ent px pz cx cy scale)
  (let* ((pos (cdr (assoc "pos" ent)))
         (size (cdr (assoc "size" ent)))
         (label (cdr (assoc "label" ent)))
         (color (cdr (assoc "color" ent)))
         (wx (list-ref pos 0))
         (wz (list-ref pos 2))
         (sw-w (* (list-ref size 0) scale))
         (sw-h (* (list-ref size 2) scale))
         (s-pos (world->screen wx wz px pz cx cy scale))
         (sx (list-ref s-pos 0))
         (sy (list-ref s-pos 1))
         (bx (- sx (/ sw-w 2.0)))
         (by (- sy (/ sw-h 2.0))))
    ;; 掩体实体与防爆斜向支撑线
    (draw-rect! bx by sw-w sw-h color)
    (draw-rect-lines! bx by sw-w sw-h 2.0 '(0.7 0.85 1.0 0.75))
    (draw-line-2d! bx by (+ bx sw-w) (+ by sw-h) 1.0 '(0.3 0.5 0.7 0.4))
    (draw-line-2d! (+ bx sw-w) by bx (+ by sw-h) 1.0 '(0.3 0.5 0.7 0.4))
    ;; 掩体标牌
    (if (and label (> (string-length label) 0))
        (let ((tw (measure-text-width label 11.0)))
          (draw-text-screen! label (- sx (/ tw 2.0)) (+ by sw-h 13.0) 11.0 '(0.7 0.85 1.0 0.8)))
        #f)))

(define (draw-plasma-bullet ent px pz cx cy scale)
  (let* ((pos (cdr (assoc "pos" ent)))
         (wx (list-ref pos 0))
         (wz (list-ref pos 2))
         (s-pos (world->screen wx wz px pz cx cy scale))
         (sx (list-ref s-pos 0))
         (sy (list-ref s-pos 1)))
    ;; 耀眼电浆光球
    (draw-circle! sx sy 4.5 '(1.0 0.9 0.25 1.0))
    (draw-circle-lines! sx sy 6.5 1.5 '(1.0 0.5 0.1 0.75))))

;; --------------------------------------------------------
;; 7. 2D 街机 HUD 战斗仪表盘与战术雷达
;; --------------------------------------------------------
(define (draw-cyber-hud player-data sw sh px pz)
  (let* ((hp (cdr (assoc "hp" player-data)))
         (max-hp (cdr (assoc "max_hp" player-data)))
         (score (cdr (assoc "score" player-data)))
         (cx (/ sw 2.0))
         (cy (/ sh 2.0)))

    ;; 7.1 顶部中央街机霓虹横幅 (CYBER TANK BATTLE 2D)
    (let* ((banner-w 360.0)
           (banner-h 38.0)
           (banner-x (- cx (/ banner-w 2.0)))
           (title "CYBER TANK BATTLE 2D")
           (tw (measure-text-width title 16.0)))
      (draw-rect! banner-x 12.0 banner-w banner-h '(0.04 0.07 0.14 0.82))
      (draw-rect-lines! banner-x 12.0 banner-w banner-h 1.5 '(0.2 0.85 1.0 0.85))
      (draw-text-screen! title (- cx (/ tw 2.0)) 35.0 16.0 '(0.1 0.95 0.85 1.0)))

    ;; 7.2 左上角战车状态与装甲仪表 (Armor & Score Dashboard)
    (let* ((dash-w 240.0)
           (dash-h 80.0)
           (hp-ratio (/ (max 0.0 hp) (max 1.0 max-hp)))
           (bar-w 180.0)
           (bar-h 14.0)
           (fill-w (* bar-w hp-ratio))
           (hp-col (if (> hp-ratio 0.4) '(0.2 0.9 0.4 0.95) '(1.0 0.3 0.2 0.95))))
      (draw-rect! 16.0 14.0 dash-w dash-h '(0.04 0.07 0.14 0.82))
      (draw-rect-lines! 16.0 14.0 dash-w dash-h 1.5 '(0.2 0.85 0.5 0.85))
      ;; 装甲值标头与指示条
      (draw-text-screen! "ARMOR INTEGRITY" 26.0 32.0 12.0 '(0.7 0.85 1.0 0.8))
      (draw-rect! 26.0 38.0 bar-w bar-h '(0.1 0.12 0.18 0.85))
      (draw-rect! 26.0 38.0 fill-w bar-h hp-col)
      (draw-rect-lines! 26.0 38.0 bar-w bar-h 1.0 '(0.4 0.6 0.8 0.85))
      (draw-text-screen! (string-append (to-string (round hp)) "%") 212.0 50.0 12.0 '(1.0 1.0 1.0 0.95))
      ;; 积分指示
      (draw-text-screen! (string-append "SCORE: " (to-string score)) 26.0 78.0 14.0 '(1.0 0.85 0.2 1.0)))

    ;; 7.3 右上角空间坐标指示
    (let ((pos-str (string-append "GRID: [" (to-string (round px)) ", " (to-string (round pz)) "]")))
      (draw-rect! (- sw 160.0) 14.0 145.0 32.0 '(0.04 0.07 0.14 0.82))
      (draw-rect-lines! (- sw 160.0) 14.0 145.0 32.0 1.0 '(0.3 0.6 0.8 0.75))
      (draw-text-screen! pos-str (- sw 148.0) 34.0 12.0 '(0.7 0.85 1.0 0.85)))

    ;; 7.4 左下角战术控制指南面板
    (let ((hint-y (- sh 68.0)))
      (draw-rect! 16.0 hint-y 260.0 52.0 '(0.03 0.05 0.1 0.75))
      (draw-rect-lines! 16.0 hint-y 260.0 52.0 1.0 '(0.2 0.4 0.6 0.6))
      (draw-text-screen! "CONTROLS: [WASD] Drive | [Mouse] Aim" 26.0 (+ hint-y 20.0) 11.0 '(0.6 0.8 1.0 0.8))
      (draw-text-screen! "CONSOLE: [ENTER] type 'fire' | 'heal' | 'help'" 26.0 (+ hint-y 38.0) 11.0 '(0.2 0.85 0.5 0.8)))

    ;; 7.5 右下角 2D 战术圆形雷达扫描仪 (Radar)
    (let* ((rx (- sw 80.0))
           (ry (- sh 80.0))
           (radius 60.0)
           (sweep-angle (* *client-time* 3.2))
           (sx (+ rx (* (cos sweep-angle) radius)))
           (sy (+ ry (* (sin sweep-angle) radius))))
      ;; 雷达圆盘与分划圈
      (draw-circle! rx ry radius '(0.03 0.06 0.12 0.82))
      (draw-circle-lines! rx ry radius 2.0 '(0.2 0.85 1.0 0.9))
      (draw-circle-lines! rx ry (* radius 0.66) 1.0 '(0.2 0.5 0.8 0.4))
      (draw-circle-lines! rx ry (* radius 0.33) 1.0 '(0.2 0.5 0.8 0.4))
      ;; 十字分划线
      (draw-line-2d! (- rx radius) ry (+ rx radius) ry 1.0 '(0.2 0.5 0.8 0.35))
      (draw-line-2d! rx (- ry radius) rx (+ ry radius) 1.0 '(0.2 0.5 0.8 0.35))
      ;; 动态扫描指针
      (draw-line-2d! rx ry sx sy 1.5 '(0.2 1.0 0.6 0.7))
      ;; 中心玩家光点与方位指北
      (draw-circle! rx ry 3.5 '(0.2 0.9 0.5 1.0))
      (draw-text-screen! "N" (- rx 4.0) (- ry radius -12.0) 11.0 '(0.2 0.85 1.0 0.85)))))

;; --------------------------------------------------------
;; 8. 客户端主渲染循环 (client-render dt)
;; --------------------------------------------------------
(define (client-render dt)
  (set! *client-time* (+ *client-time* dt))

  ;; 8.1 屏幕尺寸与自适应中心点计算
  (let* ((sw (screen-width))
         (sh (screen-height))
         (cx (/ sw 2.0))
         (cy (/ sh 2.0))
         ;; 自适应缩放系数：根据屏幕高宽计算最优比例
         (scale (max 18.0 (min 30.0 (* (/ sh 600.0) 24.0))))
         (me (get-local-player))
         (pos (if (and (list? me) (assoc "pos" me))
                  (cdr (assoc "pos" me))
                  '(0.0 0.5 0.0)))
         (px (list-ref pos 0))
         (pz (list-ref pos 2))
         (yaw (if (and (list? me) (assoc "yaw" me))
                  (cdr (assoc "yaw" me))
                  0.0)))

    ;; 8.2 保持 3D 摄像机为自顶向下俯视 (Pitch = 1.55 弧度近 90 度垂直俯视)
    (set-camera! (list px 22.0 pz) 0.0 1.55 22.0)

    ;; 8.3 绘制 2D 背景网格与边界高压线
    (draw-arena-grid sw sh px pz cx cy scale)

    ;; 8.4 遍历渲染视距内服务端同步的所有实体
    (let ((entities (get-replicated-entities)))
      (if (list? entities)
          (for-each
            (lambda (ent)
              (let ((id (cdr (assoc "id" ent)))
                    (shape (cdr (assoc "shape" ent))))
                (cond
                  ;; 其他在线玩家战车 (player- 前缀)
                  ((string-starts-with? id "player-")
                   (let* ((epos (cdr (assoc "pos" ent)))
                          (ecol (cdr (assoc "color" ent)))
                          (elabel (cdr (assoc "label" ent)))
                          (ex (list-ref epos 0))
                          (ez (list-ref epos 2))
                          (s-pt (world->screen ex ez px pz cx cy scale)))
                     (draw-tank-sprite (list-ref s-pt 0) (list-ref s-pt 1) 0.0 ecol #f 100.0 100.0 elabel)))

                  ;; 敌方自主战术无人机 (drone- 前缀)
                  ((string-starts-with? shape "drone-")
                   (draw-enemy-drone ent px pz cx cy scale))

                  ;; 可拾取道具球 (pickup)
                  ((or (string=? shape "pickup") (string-starts-with? id "pickup-"))
                   (draw-pickup-item ent px pz cx cy scale))

                  ;; 电浆炮弹 (bullet)
                  ((or (string=? shape "bullet") (string-starts-with? id "bullet-"))
                   (draw-plasma-bullet ent px pz cx cy scale))

                  ;; 静态建筑掩体与障碍物 (bunker / wall / obstacle)
                  (else
                   (draw-obstacle-bunker ent px pz cx cy scale)))))
            entities)
          #f))

    ;; 8.5 绘制本地玩家战车 (居中显示在 (cx, cy))
    (if (and (list? me) (assoc "pos" me))
        (let ((hp (cdr (assoc "hp" me)))
              (max-hp (cdr (assoc "max_hp" me))))
          (draw-tank-sprite cx cy yaw '(0.2 0.9 0.5 1.0) #t hp max-hp "COMMANDER"))
        #f)

    ;; 8.6 绘制 2D HUD 战斗仪表板与雷达小地图
    (if (list? me)
        (draw-cyber-hud me sw sh px pz)
        #f)))
