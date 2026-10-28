;; ========================================================
;; games/dungeon.scm - 地牢探险模式
;; ========================================================

(define *players* (hash))
(define *entities* '())

(define (make-entity id shape x y z sx sy sz r g b a label)
  (hash "id" id "shape" shape "x" x "y" y "z" z
        "sx" sx "sy" sy "sz" sz
        "r" r "g" g "b" b "a" a
        "label" label))

(define (init-game)
  (set! *players* (hash))
  (set! *entities*
    (list
      (make-entity "dungeon-core" "sphere" 0.0 0.5 0.0 2.0 2.0 2.0 0.8 0.1 0.9 1.0 "Dungeon Core")
      (make-entity "pillar-ne" "cube" 6.0 3.0 6.0 1.5 6.0 1.5 0.3 0.3 0.35 1.0 "Stone Pillar")
      (make-entity "pillar-nw" "cube" -6.0 3.0 6.0 1.5 6.0 1.5 0.3 0.3 0.35 1.0 "Stone Pillar")
      (make-entity "pillar-se" "cube" 6.0 3.0 -6.0 1.5 6.0 1.5 0.3 0.3 0.35 1.0 "Stone Pillar")
      (make-entity "pillar-sw" "cube" -6.0 3.0 -6.0 1.5 6.0 1.5 0.3 0.3 0.35 1.0 "Stone Pillar")))
  "Dungeon Mode Initialized")

(define (on-player-join id)
  (set! *players*
        (hash-insert *players* id
          (hash "x" 0.0 "y" 0.5 "z" -4.0 "yaw" 0.0
                "hp" 50.0 "max-hp" 50.0 "score" 0
                "logs" (list (string-append "Entered Dark Dungeon: " id)))))
  "joined")

(define (on-player-leave id)
  (set! *players* (hash-remove *players* id))
  "left")

(define (on-player-move id dx dz yaw)
  (let ((p (hash-get *players* id)))
    (if p
        (let* ((speed 0.2)
               (new-x (+ (hash-get p "x") (* dx speed)))
               (new-z (+ (hash-get p "z") (* dz speed)))
               (updated (hash-insert (hash-insert (hash-insert p "x" new-x) "z" new-z) "yaw" yaw)))
          (set! *players* (hash-insert *players* id updated))
          "moved")
        "not-found")))

(define (on-player-command id cmd)
  (let ((p (hash-get *players* id)))
    (if p
        (cond
          ((string=? cmd "torch")
           (let* ((new-ent (make-entity (string-append "torch-" id) "sphere"
                                        (hash-get p "x") 1.8 (hash-get p "z")
                                        0.6 0.6 0.6 1.0 0.5 0.1 1.0 "Torch")))
             (set! *entities* (cons new-ent *entities*))
             "System: Placed a glowing torch!"))
          (else
           (string-append "Dungeon commands: torch")))
        "player not found")))

(define (game-tick dt) #t)

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
          "\"logs\":[\"In Dungeon\"]"
          "}")
        "{}")))
