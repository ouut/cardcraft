;;; =============================================================================
;;; CardCraft - Stacklands Core Engine in Steel Scheme
;;; =============================================================================
;;; Complete, authentic console recreation of Stacklands:
;;; - Multiset recipe pattern matching
;;; - Realistic gathering, cooking, farming, refining, weapons, buildings, breeding & combat
;;; - Moon cycle survival, food consumption & starvation mechanics
;;; - Multi-pack marketplace & opening random card pools
;;; - Full-screen responsive ASCII & ANSI terminal canvas
;;; - Network multiplayer hooks & universal command dispatching
;;; =============================================================================

;;; -----------------------------------------------------------------------------
;;; 1. Global World State
;;; -----------------------------------------------------------------------------
(define *world-moon* 1)
(define *world-day-time-left* 30.0)
(define *world-day-duration* 30.0)
(define *world-gold* 5)
(define *world-status* "RUNNING")
(define *world-stacks* '())
(define *world-logs* '("Welcome to CardCraft! Type 'h' for commands."))
(define *next-stack-id* 1)
(define *tui-input-buffer* "")
(define *tui-last-feedback* "OK")

;;; -----------------------------------------------------------------------------
;;; 2. ANSI Styling Escape Constants (VT100)
;;; -----------------------------------------------------------------------------
(define ESC (string (integer->char 27)))
(define ANSI-RESET   (string-append ESC "[0m"))
(define ANSI-BOLD    (string-append ESC "[1m"))
(define ANSI-DIM     (string-append ESC "[2m"))
(define ANSI-RED     (string-append ESC "[31m"))
(define ANSI-GREEN   (string-append ESC "[32m"))
(define ANSI-YELLOW  (string-append ESC "[33m"))
(define ANSI-BLUE    (string-append ESC "[34m"))
(define ANSI-MAGENTA (string-append ESC "[35m"))
(define ANSI-CYAN    (string-append ESC "[36m"))
(define ANSI-WHITE   (string-append ESC "[37m"))
(define ANSI-GRAY    (string-append ESC "[90m"))

;;; -----------------------------------------------------------------------------
;;; 3. String & Multiset Helpers
;;; -----------------------------------------------------------------------------
(define (starts-with? str prefix)
  (let ([plen (string-length prefix)]
        [slen (string-length str)])
    (and (>= slen plen)
         (equal? (substring str 0 plen) prefix))))

(define (join-strings lst sep)
  (cond
    [(null? lst) ""]
    [(null? (cdr lst)) (car lst)]
    [else
     (let loop ([rem (cdr lst)] [acc (car lst)])
       (if (null? rem)
           acc
           (loop (cdr rem) (string-append acc sep (car rem)))))]))

(define (split-spaces str)
  (let ([len (string-length str)])
    (let loop ([i 0] [cur '()] [words '()])
      (if (>= i len)
          (let ([last (list->string (reverse cur))])
            (reverse (if (equal? last "") words (cons last words))))
          (let ([ch (string-ref str i)])
            (if (or (char=? ch #\space) (char=? ch #\tab) (char=? ch #\newline))
                (let ([tok (list->string (reverse cur))])
                  (loop (+ i 1) '() (if (equal? tok "") words (cons tok words))))
                (loop (+ i 1) (cons ch cur) words)))))))

(define (remove-one item lst)
  (cond
    [(null? lst) '()]
    [(equal? (car lst) item) (cdr lst)]
    [else (cons (car lst) (remove-one item (cdr lst)))]))

(define (multiset-subset? sub super)
  (cond
    [(null? sub) #t]
    [else
     (let ([item (car sub)])
       (if (member item super)
           (multiset-subset? (cdr sub) (remove-one item super))
           #f))]))

(define (multiset-diff super sub)
  (cond
    [(null? sub) super]
    [else (multiset-diff (remove-one (car sub) super) (cdr sub))]))

(define (random-choice lst)
  (if (null? lst)
      #f
      (let* ([len (length lst)]
             [idx (system-utils-random-int 0 (- len 1))])
        (list-ref lst idx))))

(define (escape-json-str s)
  (let ([len (string-length s)])
    (let loop ([i 0] [acc '()])
      (if (>= i len)
          (list->string (reverse acc))
          (let ([ch (string-ref s i)])
            (cond
              [(char=? ch #\\) (loop (+ i 1) (cons #\\ (cons #\\ acc)))]
              [(char=? ch #\") (loop (+ i 1) (cons #\" (cons #\\ acc)))]
              [(char=? ch #\newline) (loop (+ i 1) (cons #\n (cons #\\ acc)))]
              [(char=? ch (integer->char 27))
               (loop (+ i 1) (cons #\b (cons #\1 (cons #\0 (cons #\0 (cons #\u (cons #\\ acc)))))))]
              [else (loop (+ i 1) (cons ch acc))]))))))

(define (add-log! msg)
  (set! *world-logs* (cons msg *world-logs*)))

;;; -----------------------------------------------------------------------------
;;; 4. Card Properties: Colors, Food Values & Sell Values
;;; -----------------------------------------------------------------------------
(define (colorize-card name)
  (cond
    [(member name '("Villager" "Militia" "Swordsman" "Dog"))
     (string-append ANSI-CYAN name ANSI-RESET)]
    [(equal? name "Baby")
     (string-append ANSI-MAGENTA name ANSI-RESET)]
    [(member name '("Berry" "Apple" "Mushroom" "Carrot" "CookedBerry" "CookedMeat" "FruitSalad" "Stew"))
     (string-append ANSI-GREEN name ANSI-RESET)]
    [(member name '("BerryBush" "Tree" "Soil" "Garden" "Farm"))
     (string-append ANSI-GREEN ANSI-BOLD name ANSI-RESET)]
    [(member name '("Wood" "Stone" "Flint" "Stick" "Plank" "Brick" "IronOre" "IronBar" "Rock" "IronDeposit"))
     (string-append ANSI-YELLOW name ANSI-RESET)]
    [(equal? name "Coin")
     (string-append ANSI-YELLOW ANSI-BOLD name ANSI-RESET)]
    [(member name '("Campfire" "House" "Shed" "Warehouse" "Quarry" "LumberCamp" "Smelter"))
     (string-append ANSI-MAGENTA ANSI-BOLD name ANSI-RESET)]
    [(member name '("Rat" "Goblin" "Skeleton" "Wolf"))
     (string-append ANSI-RED ANSI-BOLD name ANSI-RESET)]
    [(member name '("Spear" "Sword" "Bone" "RawMeat"))
     (string-append ANSI-RED name ANSI-RESET)]
    [(starts-with? name "Pack:")
     (string-append ANSI-WHITE ANSI-BOLD name ANSI-RESET)]
    [(starts-with? name "Idea:")
     (string-append ANSI-BLUE ANSI-BOLD name ANSI-RESET)]
    [else name]))

(define (card-food-value card)
  (cond
    [(equal? card "Berry") 1]
    [(equal? card "Apple") 1]
    [(equal? card "Mushroom") 1]
    [(equal? card "Carrot") 1]
    [(equal? card "CookedBerry") 2]
    [(equal? card "CookedMeat") 3]
    [(equal? card "FruitSalad") 3]
    [(equal? card "Stew") 4]
    [else 0]))

(define (card-sell-value card)
  (cond
    [(equal? card "Coin") 1]
    [(equal? card "Berry") 1]
    [(equal? card "Apple") 1]
    [(equal? card "Mushroom") 1]
    [(equal? card "Carrot") 1]
    [(equal? card "CookedBerry") 2]
    [(equal? card "CookedMeat") 3]
    [(equal? card "FruitSalad") 4]
    [(equal? card "Stew") 5]
    [(equal? card "Wood") 1]
    [(equal? card "Stone") 1]
    [(equal? card "Flint") 1]
    [(equal? card "Stick") 1]
    [(equal? card "Bone") 1]
    [(equal? card "RawMeat") 1]
    [(equal? card "Plank") 2]
    [(equal? card "Brick") 2]
    [(equal? card "IronOre") 2]
    [(equal? card "Spear") 2]
    [(equal? card "IronBar") 4]
    [(equal? card "Sword") 5]
    [(equal? card "Soil") 1]
    [(equal? card "BerryBush") 2]
    [(equal? card "Tree") 2]
    [(equal? card "Rock") 2]
    [(equal? card "IronDeposit") 3]
    [(equal? card "Campfire") 3]
    [(equal? card "Shed") 4]
    [(equal? card "House") 5]
    [(equal? card "Garden") 4]
    [(equal? card "Farm") 8]
    [(equal? card "Quarry") 6]
    [(equal? card "LumberCamp") 6]
    [(equal? card "Smelter") 8]
    [(equal? card "Warehouse") 10]
    [(starts-with? card "Idea:") 1]
    [else 0]))

;;; -----------------------------------------------------------------------------
;;; 5. Authentic Stacklands Recipe Table (35+ Recipes)
;;; Format: (Name Inputs Duration Outputs ConsumedInputs)
;;; -----------------------------------------------------------------------------
(define *recipes*
  '(;; --- 1. Gathering & Harvest ---
    ("Harvest Berry" ("Villager" "BerryBush") 4.0 ("Berry") ())
    ("Chop Tree" ("Villager" "Tree") 5.0 ("Wood") ())
    ("Mine Rock" ("Villager" "Rock") 5.0 ("Stone") ())
    ("Mine Iron" ("Villager" "IronDeposit") 6.0 ("IronOre") ())

    ;; --- 2. Cooking (Campfire) ---
    ("Cook Berry" ("Campfire" "Berry") 3.0 ("CookedBerry") ("Berry"))
    ("Cook Meat" ("Campfire" "RawMeat") 4.0 ("CookedMeat") ("RawMeat"))
    ("Fruit Salad" ("Campfire" "Berry" "Apple") 4.0 ("FruitSalad") ("Berry" "Apple"))
    ("Meat Stew" ("Campfire" "CookedMeat" "Carrot") 5.0 ("Stew") ("CookedMeat" "Carrot"))

    ;; --- 3. Farming & Planting (Sustainable Agriculture) ---
    ("Plant Berry" ("Villager" "Soil" "Berry") 6.0 ("BerryBush") ("Berry"))
    ("Plant Tree" ("Villager" "Soil" "Apple") 8.0 ("Tree") ("Apple"))
    ("Farm Carrots" ("Villager" "Farm" "Carrot") 5.0 ("Carrot" "Carrot") ())
    ("Garden Berries" ("Villager" "Garden" "Berry") 5.0 ("Berry" "Berry") ())

    ;; --- 4. Basic Crafting & Refining ---
    ("Make Stick" ("Villager" "Wood") 3.0 ("Stick" "Stick") ("Wood"))
    ("Make Plank" ("Villager" "Wood" "Wood") 4.0 ("Plank") ("Wood" "Wood"))
    ("Make Brick" ("Villager" "Stone" "Stone") 4.0 ("Brick") ("Stone" "Stone"))
    ("Smelt Iron" ("Smelter" "IronOre" "Wood") 5.0 ("IronBar") ("IronOre" "Wood"))

    ;; --- 5. Weapons & Combat Training ---
    ("Craft Spear" ("Villager" "Wood" "Flint") 4.0 ("Spear") ("Wood" "Flint"))
    ("Equip Militia" ("Villager" "Spear") 2.0 ("Militia") ("Villager" "Spear"))
    ("Craft Sword" ("Villager" "IronBar" "Stick") 5.0 ("Sword") ("IronBar" "Stick"))
    ("Train Swordsman" ("Villager" "Sword") 3.0 ("Swordsman") ("Villager" "Sword"))
    ("Tame Wolf" ("Wolf" "Bone") 5.0 ("Dog") ("Wolf" "Bone"))

    ;; --- 6. Buildings & Infrastructure ---
    ("Build Campfire" ("Villager" "Wood" "Wood" "Stone") 6.0 ("Campfire") ("Wood" "Wood" "Stone"))
    ("Build Shed" ("Villager" "Wood" "Stone" "Stone") 6.0 ("Shed") ("Wood" "Stone" "Stone"))
    ("Build House" ("Villager" "Wood" "Wood" "Stone" "Stone") 8.0 ("House") ("Wood" "Wood" "Stone" "Stone"))
    ("Build Garden" ("Villager" "Soil" "Wood" "Stone") 6.0 ("Garden") ("Soil" "Wood" "Stone"))
    ("Build Farm" ("Villager" "Garden" "Plank" "Brick") 8.0 ("Farm") ("Garden" "Plank" "Brick"))
    ("Build Quarry" ("Villager" "Stone" "Stone" "Wood") 8.0 ("Quarry") ("Stone" "Stone" "Wood"))
    ("Build LumberCamp" ("Villager" "Wood" "Wood" "Stone") 8.0 ("LumberCamp") ("Wood" "Wood" "Stone"))
    ("Build Smelter" ("Villager" "Brick" "Brick" "Flint") 8.0 ("Smelter") ("Brick" "Brick" "Flint"))
    ("Build Warehouse" ("Villager" "Plank" "Brick" "IronBar") 10.0 ("Warehouse") ("Plank" "Brick" "IronBar"))

    ;; --- 7. Industrial Automation ---
    ("Work Quarry" ("Villager" "Quarry") 5.0 ("Stone") ())
    ("Work LumberCamp" ("Villager" "LumberCamp") 5.0 ("Wood") ())

    ;; --- 8. Population & Growth ---
    ("Breed Villager" ("Villager" "Villager" "House") 10.0 ("Baby") ())
    ("Grow Baby" ("Baby") 10.0 ("Villager") ("Baby"))
    ("Grow Baby House" ("Baby" "House") 6.0 ("Villager") ("Baby"))

    ;; --- 9. Combat Battles ---
    ("Fight Rat" ("Villager" "Rat") 4.0 ("RawMeat" "Coin") ("Rat"))
    ("Hunt Rat" ("Militia" "Rat") 2.0 ("RawMeat" "Coin") ("Rat"))
    ("Hunt Rat Dog" ("Dog" "Rat") 2.0 ("RawMeat") ("Rat"))
    ("Fight Goblin" ("Militia" "Goblin") 4.0 ("Coin" "Bone") ("Goblin"))
    ("Slay Goblin" ("Swordsman" "Goblin") 2.5 ("Coin" "Spear") ("Goblin"))
    ("Fight Skeleton" ("Swordsman" "Skeleton") 4.0 ("Sword" "Bone") ("Skeleton"))
    ("Hunt Wolf" ("Militia" "Wolf") 5.0 ("RawMeat" "Bone") ("Wolf"))
    ("Slay Wolf" ("Swordsman" "Wolf") 3.0 ("RawMeat" "Bone") ("Wolf"))))

(define (find-matching-recipe cards)
  (let loop ([r-list *recipes*])
    (if (null? r-list)
        #f
        (let* ([r (car r-list)]
               [inputs (cadr r)])
          (if (multiset-subset? inputs cards)
              r
              (loop (cdr r-list)))))))

;;; -----------------------------------------------------------------------------
;;; 6. Stack & World Entity Accessors
;;; -----------------------------------------------------------------------------
;; Stack struct: (id cards progress total-time recipe-name status)
(define (make-stack id cards progress total-time recipe-name status)
  (list id cards progress total-time recipe-name status))

(define (stack-id s) (car s))
(define (stack-cards s) (cadr s))
(define (stack-progress s) (caddr s))
(define (stack-total-time s) (cadddr s))
(define (stack-recipe-name s) (list-ref s 4))
(define (stack-status s) (list-ref s 5))

(define (refresh-stack-recipe s)
  (let* ([id (stack-id s)]
         [cards (stack-cards s)]
         [rec (find-matching-recipe cards)])
    (if rec
        (let ([name (car rec)]
              [dur (caddr rec)])
          (if (and (equal? (stack-recipe-name s) name)
                   (number? (stack-progress s)))
              s
              (make-stack id cards 0.0 dur name "Crafting")))
        (make-stack id cards '() '() '() "Idle"))))

(define (total-card-count)
  (let loop ([stks *world-stacks*] [acc 0])
    (if (null? stks)
        acc
        (loop (cdr stks) (+ acc (length (stack-cards (car stks))))))))

(define (calculate-card-limit)
  ;; Base 15 + 5 per Shed + 10 per Warehouse
  (let loop ([stks *world-stacks*] [extra 0])
    (if (null? stks)
        (+ 15 extra)
        (let ([cards (stack-cards (car stks))])
          (let in-cards ([c-list cards] [sub-acc 0])
            (cond
              [(null? c-list) (loop (cdr stks) (+ extra sub-acc))]
              [(equal? (car c-list) "Shed") (in-cards (cdr c-list) (+ sub-acc 5))]
              [(equal? (car c-list) "Warehouse") (in-cards (cdr c-list) (+ sub-acc 10))]
              [else (in-cards (cdr c-list) sub-acc)]))))))

(define (count-villagers)
  (let loop ([stks *world-stacks*] [acc 0])
    (if (null? stks)
        acc
        (let ([cards (stack-cards (car stks))])
          (let in-cards ([c-list cards] [sub-acc 0])
            (cond
              [(null? c-list) (loop (cdr stks) (+ acc sub-acc))]
              [(member (car c-list) '("Villager" "Militia" "Swordsman"))
               (in-cards (cdr c-list) (+ sub-acc 1))]
              [else (in-cards (cdr c-list) sub-acc)]))))))

(define (total-food-value)
  (let loop ([stks *world-stacks*] [acc 0])
    (if (null? stks)
        acc
        (let ([cards (stack-cards (car stks))])
          (let in-cards ([c-list cards] [sub-acc 0])
            (if (null? c-list)
                (loop (cdr stks) (+ acc sub-acc))
                (in-cards (cdr c-list) (+ sub-acc (card-food-value (car c-list))))))))))

(define (get-next-id!)
  (let ([id *next-stack-id*])
    (set! *next-stack-id* (+ *next-stack-id* 1))
    id))

(define (spawn-card! card)
  (let ([id (get-next-id!)])
    (let ([new-s (refresh-stack-recipe (make-stack id (list card) '() '() '() "Idle"))])
      (set! *world-stacks* (append *world-stacks* (list new-s))))))

(define (find-stack id)
  (let loop ([stks *world-stacks*])
    (cond
      [(null? stks) #f]
      [(= (stack-id (car stks)) id) (car stks)]
      [else (loop (cdr stks))])))

(define (remove-stack id)
  (set! *world-stacks*
        (let loop ([stks *world-stacks*])
          (cond
            [(null? stks) '()]
            [(= (stack-id (car stks)) id) (cdr stks)]
            [else (cons (car stks) (loop (cdr stks)))]))))

(define (update-stack! s)
  (set! *world-stacks*
        (let loop ([stks *world-stacks*])
          (cond
            [(null? stks) '()]
            [(= (stack-id (car stks)) (stack-id s)) (cons s (cdr stks))]
            [else (cons (car stks) (loop (cdr stks)))]))))

;;; -----------------------------------------------------------------------------
;;; 7. Lifecycle & Simulation Ticks (game-start! & game-update!)
;;; -----------------------------------------------------------------------------
(define (game-start!)
  (set! *world-moon* 1)
  (set! *world-day-time-left* 30.0)
  (set! *world-day-duration* 30.0)
  (set! *world-gold* 5)
  (set! *world-status* "RUNNING")
  (set! *next-stack-id* 1)
  (set! *world-logs* '("Game started! Stack Villager onto BerryBush (s 1 2)."))
  (set! *world-stacks* '())
  ;; Opening 5 starter slots
  (spawn-card! "Villager")   ; 1
  (spawn-card! "BerryBush")  ; 2
  (spawn-card! "Berry")      ; 3
  (spawn-card! "Wood")       ; 4
  (spawn-card! "Coin")       ; 5
  "Initialized")

(define (game-update! dt)
  (if (equal? *world-status* "RUNNING")
      (begin
        ;; 1. Update stack progress & outputs
        (let ([pending-spawns '()])
          (define (tick-stacks stks)
            (if (null? stks)
                '()
                (let* ([s (car stks)]
                       [prog (stack-progress s)])
                  (if (and (number? prog) (string? (stack-recipe-name s)))
                      (let* ([new-prog (+ prog dt)]
                             [total (stack-total-time s)])
                        (if (>= new-prog total)
                            ;; Recipe completed!
                            (let* ([rec (find-matching-recipe (stack-cards s))]
                                   [rec-outputs (list-ref rec 3)]
                                   [rec-consumed (list-ref rec 4)]
                                   [remaining (multiset-diff (stack-cards s) rec-consumed)])
                              (add-log! (string-append "Produced: " (join-strings rec-outputs ", ")))
                              (set! pending-spawns (append pending-spawns rec-outputs))
                              (if (null? remaining)
                                  (tick-stacks (cdr stks))
                                  (let ([refreshed (refresh-stack-recipe (make-stack (stack-id s) remaining '() '() '() "Idle"))])
                                    (cons refreshed (tick-stacks (cdr stks))))))
                            ;; In progress
                            (cons (make-stack (stack-id s) (stack-cards s) new-prog total (stack-recipe-name s) "Crafting")
                                  (tick-stacks (cdr stks)))))
                      (cons s (tick-stacks (cdr stks)))))))
          (set! *world-stacks* (tick-stacks *world-stacks*))
          (for-each spawn-card! pending-spawns))

        ;; 2. Day & Moon Timer Progression
        (set! *world-day-time-left* (- *world-day-time-left* dt))
        (if (<= *world-day-time-left* 0.0)
            (begin
              (let* ([req (count-villagers)]
                     [available (total-food-value)])
                (if (< available req)
                    (let ([starved (- req available)])
                      (add-log! (string-append "WARNING: Food shortage! " (number->string starved) " villager(s) starved!"))
                      ;; Deduct remaining food & remove starved villagers
                      (let ([to-remove starved])
                        (define (kill-villagers stks)
                          (if (or (null? stks) (<= to-remove 0))
                              stks
                              (let* ([s (car stks)]
                                     [cards (stack-cards s)])
                                (if (member "Villager" cards)
                                    (begin
                                      (set! to-remove (- to-remove 1))
                                      (let ([rem (remove-one "Villager" cards)])
                                        (if (null? rem)
                                            (kill-villagers (cdr stks))
                                            (cons (refresh-stack-recipe (make-stack (stack-id s) rem '() '() '() "Idle"))
                                                  (kill-villagers (cdr stks))))))
                                    (cons s (kill-villagers (cdr stks)))))))
                        (set! *world-stacks* (kill-villagers *world-stacks*))))
                    (begin
                      (add-log! (string-append "Moon ended! Consumed " (number->string req) " food."))
                      ;; Deduct required food points
                      (let ([to-consume req])
                        (define (consume-from-stacks stks)
                          (if (or (null? stks) (<= to-consume 0))
                              stks
                              (let* ([s (car stks)]
                                     [cards (stack-cards s)])
                                (define (consume-cards c-list)
                                  (cond
                                    [(null? c-list) '()]
                                    [(<= to-consume 0) c-list]
                                    [(> (card-food-value (car c-list)) 0)
                                     (set! to-consume (- to-consume (card-food-value (car c-list))))
                                     (consume-cards (cdr c-list))]
                                    [else (cons (car c-list) (consume-cards (cdr c-list)))]))
                                (let ([rem (consume-cards cards)])
                                  (if (null? rem)
                                      (consume-from-stacks (cdr stks))
                                      (cons (refresh-stack-recipe (make-stack (stack-id s) rem '() '() '() "Idle"))
                                            (consume-from-stacks (cdr stks))))))))
                        (set! *world-stacks* (consume-from-stacks *world-stacks*))))))
              (set! *world-moon* (+ *world-moon* 1))
              (set! *world-day-time-left* *world-day-duration*)
              (add-log! (string-append "=== Moon " (number->string *world-moon*) " Begun! ===")))))
      #f))

;;; -----------------------------------------------------------------------------
;;; 8. CAD Command Dispatcher (game-input)
;;; -----------------------------------------------------------------------------
(define (game-input . rest)
  (let* ([has-cid? (> (length rest) 1)]
         [cid (if has-cid? (car rest) #f)]
         [tokens (if has-cid? (cadr rest) (car rest))])
    (if (null? tokens)
        "Empty command"
        (let ([verb (car tokens)]
              [args (cdr tokens)])
          (cond
            ;; 1. Stack: s <from> <to>
            [(or (equal? verb "s") (equal? verb "stack"))
             (if (< (length args) 2)
                 "Usage: s <from-id> <to-id>"
                 (let ([from-id (string->number (car args))]
                       [to-id (string->number (cadr args))])
                   (let ([s-from (find-stack from-id)]
                         [s-to (find-stack to-id)])
                     (cond
                       [(not s-from) (string-append "Stack " (car args) " not found.")]
                       [(not s-to) (string-append "Stack " (cadr args) " not found.")]
                       [(= from-id to-id) "Cannot stack onto itself."]
                       [else
                        (let* ([merged (append (stack-cards s-to) (stack-cards s-from))]
                               [new-to (refresh-stack-recipe (make-stack to-id merged '() '() '() "Idle"))])
                          (remove-stack from-id)
                          (update-stack! new-to)
                          (add-log! (string-append "Stacked [" (car args) "] onto [" (cadr args) "]."))
                          "OK")]))))]

            ;; 2. Unstack: u <id>
            [(or (equal? verb "u") (equal? verb "unstack"))
             (if (< (length args) 1)
                 "Usage: u <slot-id>"
                 (let ([id (string->number (car args))])
                   (let ([s (find-stack id)])
                     (if (not s)
                         (string-append "Stack " (car args) " not found.")
                         (let ([cards (stack-cards s)])
                           (if (<= (length cards) 1)
                               "Stack has only 1 card, cannot unstack."
                               (let* ([top-card (car (reverse cards))]
                                      [rem-cards (reverse (cdr (reverse cards)))]
                                      [updated (refresh-stack-recipe (make-stack id rem-cards '() '() '() "Idle"))])
                                 (update-stack! updated)
                                 (spawn-card! top-card)
                                 (add-log! (string-append "Popped " top-card " from [" (car args) "] to new slot."))
                                 "OK")))))))]

            ;; 3. Move: m <from> <to>
            [(or (equal? verb "m") (equal? verb "move"))
             (if (< (length args) 2)
                 "Usage: m <from-id> <to-id>"
                 (let ([from-id (string->number (car args))]
                       [to-id (string->number (cadr args))])
                   (let ([s-from (find-stack from-id)]
                         [s-to (find-stack to-id)])
                     (cond
                       [(not s-from) (string-append "Source stack " (car args) " not found.")]
                       [s-to "Target slot already occupied. Use 's' to stack."]
                       [else
                        (let ([moved (make-stack to-id (stack-cards s-from) (stack-progress s-from)
                                                 (stack-total-time s-from) (stack-recipe-name s-from) (stack-status s-from))])
                          (remove-stack from-id)
                          (set! *world-stacks* (append *world-stacks* (list moved)))
                          (add-log! (string-append "Moved [" (car args) "] to [" (cadr args) "]."))
                          "OK")]))))]

            ;; 4. Buy Pack: b <pack-name>
            [(or (equal? verb "b") (equal? verb "buy"))
             (if (< (length args) 1)
                 "Usage: b <basic|food|struct|explore>"
                 (let ([pack (car args)])
                   (cond
                     [(or (equal? pack "basic") (equal? pack "1"))
                      (if (< *world-gold* 3)
                          "Not enough gold! Basic Pack costs 3G."
                          (begin
                            (set! *world-gold* (- *world-gold* 3))
                            (spawn-card! "Pack:Basic")
                            (add-log! "Purchased Basic Pack (3G).")
                            "OK"))]
                     [(or (equal? pack "food") (equal? pack "2"))
                      (if (< *world-gold* 5)
                          "Not enough gold! Food Pack costs 5G."
                          (begin
                            (set! *world-gold* (- *world-gold* 5))
                            (spawn-card! "Pack:Food")
                            (add-log! "Purchased Food Pack (5G).")
                            "OK"))]
                     [(or (equal? pack "struct") (equal? pack "structures") (equal? pack "3"))
                      (if (< *world-gold* 10)
                          "Not enough gold! Structures Pack costs 10G."
                          (begin
                            (set! *world-gold* (- *world-gold* 10))
                            (spawn-card! "Pack:Structures")
                            (add-log! "Purchased Structures Pack (10G).")
                            "OK"))]
                     [(or (equal? pack "explore") (equal? pack "combat") (equal? pack "4"))
                      (if (< *world-gold* 12)
                          "Not enough gold! Exploration Pack costs 12G."
                          (begin
                            (set! *world-gold* (- *world-gold* 12))
                            (spawn-card! "Pack:Exploration")
                            (add-log! "Purchased Exploration Pack (12G).")
                            "OK"))]
                     [else "Unknown pack. Available: basic (3G), food (5G), struct (10G), explore (12G)"])))]

            ;; 5. Open Pack: o <slot-id>
            [(or (equal? verb "o") (equal? verb "open"))
             (if (< (length args) 1)
                 "Usage: o <slot-id>"
                 (let ([id (string->number (car args))])
                   (let ([s (find-stack id)])
                     (if (not s)
                         (string-append "Stack " (car args) " not found.")
                         (let ([cards (stack-cards s)])
                           (cond
                             [(member "Pack:Basic" cards)
                              (let ([rem (remove-one "Pack:Basic" cards)])
                                (if (null? rem)
                                    (remove-stack id)
                                    (update-stack! (refresh-stack-recipe (make-stack id rem '() '() '() "Idle"))))
                                (let* ([pool '("Wood" "Stone" "BerryBush" "Berry" "Coin" "Soil" "Idea:Campfire")]
                                       [c1 (random-choice pool)]
                                       [c2 (random-choice pool)]
                                       [c3 (random-choice pool)])
                                  (spawn-card! c1) (spawn-card! c2) (spawn-card! c3)
                                  (add-log! (string-append "Opened Basic Pack! Got " c1 ", " c2 ", " c3 "."))
                                  "OK"))]
                             [(member "Pack:Food" cards)
                              (let ([rem (remove-one "Pack:Food" cards)])
                                (if (null? rem)
                                    (remove-stack id)
                                    (update-stack! (refresh-stack-recipe (make-stack id rem '() '() '() "Idle"))))
                                (let* ([pool '("Berry" "Apple" "Carrot" "Mushroom" "Soil" "BerryBush" "Idea:Garden")]
                                       [c1 (random-choice pool)]
                                       [c2 (random-choice pool)]
                                       [c3 (random-choice pool)])
                                  (spawn-card! c1) (spawn-card! c2) (spawn-card! c3)
                                  (add-log! (string-append "Opened Food Pack! Got " c1 ", " c2 ", " c3 "."))
                                  "OK"))]
                             [(member "Pack:Structures" cards)
                              (let ([rem (remove-one "Pack:Structures" cards)])
                                (if (null? rem)
                                    (remove-stack id)
                                    (update-stack! (refresh-stack-recipe (make-stack id rem '() '() '() "Idle"))))
                                (let* ([pool '("Wood" "Stone" "Flint" "Plank" "Brick" "Idea:House" "Idea:Shed" "Idea:Quarry")]
                                       [c1 (random-choice pool)]
                                       [c2 (random-choice pool)]
                                       [c3 (random-choice pool)])
                                  (spawn-card! c1) (spawn-card! c2) (spawn-card! c3)
                                  (add-log! (string-append "Opened Structures Pack! Got " c1 ", " c2 ", " c3 "."))
                                  "OK"))]
                             [(member "Pack:Exploration" cards)
                              (let ([rem (remove-one "Pack:Exploration" cards)])
                                (if (null? rem)
                                    (remove-stack id)
                                    (update-stack! (refresh-stack-recipe (make-stack id rem '() '() '() "Idle"))))
                                (let* ([pool '("Rat" "Goblin" "Wolf" "IronOre" "Flint" "Bone" "Idea:Spear" "Coin")]
                                       [c1 (random-choice pool)]
                                       [c2 (random-choice pool)]
                                       [c3 (random-choice pool)])
                                  (spawn-card! c1) (spawn-card! c2) (spawn-card! c3)
                                  (add-log! (string-append "Opened Exploration Pack! Encountered " c1 ", " c2 ", " c3 "!"))
                                  "OK"))]
                             [else "No unopened pack in this stack."]))))))]

            ;; 6. Sell: x <slot-id>
            [(or (equal? verb "x") (equal? verb "sell"))
             (if (< (length args) 1)
                 "Usage: x <slot-id>"
                 (let ([id (string->number (car args))])
                   (let ([s (find-stack id)])
                     (if (not s)
                         (string-append "Stack " (car args) " not found.")
                         (let* ([cards (stack-cards s)]
                                [top (car (reverse cards))]
                                [val (card-sell-value top)])
                           (if (<= val 0)
                               (string-append "Cannot sell " top " (val 0G).")
                               (let* ([rem (reverse (cdr (reverse cards)))]
                                      [updated (refresh-stack-recipe (make-stack id rem '() '() '() "Idle"))])
                                 (set! *world-gold* (+ *world-gold* val))
                                 (if (null? rem)
                                     (remove-stack id)
                                     (update-stack! updated))
                                 (add-log! (string-append "Sold " top " for " (number->string val) "G."))
                                 "OK")))))))]

            ;; 7. Wait: w [seconds]
            [(or (equal? verb "w") (equal? verb "wait"))
             (let ([dt (if (null? args) 1.0 (string->number (car args)))])
               (game-update! dt)
               (add-log! (string-append "Waited " (number->string dt) "s."))
               "OK")]

            ;; 8. Pause: p
            [(or (equal? verb "p") (equal? verb "pause"))
             (if (equal? *world-status* "RUNNING")
                 (begin (set! *world-status* "PAUSED") (add-log! "Game paused.") "PAUSED")
                 (begin (set! *world-status* "RUNNING") (add-log! "Game resumed.") "RUNNING"))]

            ;; 9. Help: h
            [(or (equal? verb "h") (equal? verb "help") (equal? verb "?"))
             (add-log! "Commands: s <from> <to> | u <id> | m <from> <to> | b <pack> | o <id> | x <id> | w [s] | p")
             "OK"]

            [else
             (string-append "Unknown CAD command: " verb ". Type 'h' for help.")])))))

;;; -----------------------------------------------------------------------------
;;; 9. Network Hooks, Multiplayer State & Unified Router
;;; -----------------------------------------------------------------------------
(define *net-clients* '())

(define (find-net-client cid)
  (assoc cid *net-clients*))

(define (get-net-client-name cid)
  (let ([c (find-net-client cid)])
    (if c (cadr c) (string-append "Player_" (number->string cid)))))

(define (find-net-client-by-name name)
  (let loop ([cs *net-clients*])
    (cond
      [(null? cs) #f]
      [(equal? (cadr (car cs)) name) (car cs)]
      [else (loop (cdr cs))])))

(define (update-net-client-name! cid new-name)
  (set! *net-clients*
        (map (lambda (cl)
               (if (= (car cl) cid)
                   (list cid new-name)
                   cl))
             *net-clients*)))

(define (broadcast! msg)
  (for-each (lambda (c) (system-net-send! (car c) msg)) *net-clients*))

(define (net-on-connect! cid)
  (let ([default-name (string-append "Player_" (number->string cid))])
    (set! *net-clients* (cons (list cid default-name) *net-clients*))
    (system-net-send! cid (string-append "=== Welcome to CardCraft! You are " default-name " ==="))
    (broadcast! (string-append ">>> " default-name " connected to the server."))
    (add-log! (string-append "Player #" (number->string cid) " connected."))
    "OK"))

(define (net-on-disconnect! cid)
  (let ([cl (find-net-client cid)])
    (if cl
        (let ([name (cadr cl)])
          (set! *net-clients*
                (let loop ([cs *net-clients*])
                  (cond
                    [(null? cs) '()]
                    [(= (car (car cs)) cid) (cdr cs)]
                    [else (cons (car cs) (loop (cdr cs)))])))
          (broadcast! (string-append "<<< " name " disconnected from the server."))
          "OK")
        "OK")))

(define (dispatch-command! cid msg-str)
  (let* ([c (find-net-client cid)]
         [name (if c (cadr c) (string-append "Player_" (number->string cid)))]
         [tokens (split-spaces msg-str)])
    (if (null? tokens)
        "Empty command"
        (let ([verb (car tokens)]
              [args (cdr tokens)])
          (cond
            ;; 1. Nickname
            [(equal? verb "nick")
             (if (null? args)
                 (begin
                   (system-net-send! cid "Usage: nick <new-name>")
                   "Usage: nick <new-name>")
                 (let ([new-name (car args)])
                   (if (find-net-client-by-name new-name)
                       (begin
                         (system-net-send! cid (string-append "Name '" new-name "' is already taken."))
                         "Name taken")
                       (begin
                         (update-net-client-name! cid new-name)
                         (broadcast! (string-append "*** " name " is now known as " new-name "."))
                         "OK"))))]

            ;; 2. Public Chat
            [(equal? verb "say")
             (if (null? args)
                 (begin
                   (system-net-send! cid "Usage: say <message...>")
                   "Usage: say <message...>")
                 (let ([chat-text (join-strings args " ")])
                   (broadcast! (string-append "[" name "]: " chat-text))
                   (add-log! (string-append "[" name "]: " chat-text))
                   "OK"))]

            ;; 3. Whisper (Only when 'w' has a non-numeric target name)
            [(or (equal? verb "tell") (equal? verb "whisper")
                 (and (equal? verb "w") (not (null? args)) (not (string->number (car args)))))
             (if (< (length args) 2)
                 (begin
                   (system-net-send! cid "Usage: w <username> <message...>")
                   "Usage: w <username> <message...>")
                 (let* ([target-name (car args)]
                        [chat-text (join-strings (cdr args) " ")]
                        [target (find-net-client-by-name target-name)])
                   (if (not target)
                       (begin
                         (system-net-send! cid (string-append "User '" target-name "' not found."))
                         "User not found")
                       (let ([target-cid (car target)])
                         (system-net-send! target-cid (string-append "[Private from " name "]: " chat-text))
                         (system-net-send! cid (string-append "[Private to " target-name "]: " chat-text))
                         "OK"))))]

            ;; 4. Who / Users list
            [(or (equal? verb "who") (equal? verb "users") (equal? verb "online"))
             (let* ([names (map (lambda (cl) (cadr cl)) *net-clients*)]
                    [msg (string-append "Online users (" (number->string (length names)) "): " (join-strings names ", "))])
               (system-net-send! cid msg)
               msg)]

            ;; 5. Otherwise: CAD Game Command (s 1 2, b basic, u 1, etc.)
            [else
             (let ([res (game-input cid tokens)])
               (system-net-send! cid (string-append "Result: " res))
               res)])))))

(define (net-on-message! cid msg-str)
  (cond
    ;; 1. Local TUI Keyboard Events
    [(starts-with? msg-str "key:")
     (let ([k (substring msg-str 4 (string-length msg-str))])
       (cond
         [(equal? k "enter")
          (let ([cmd *tui-input-buffer*])
            (set! *tui-input-buffer* "")
            (if (equal? cmd "")
                "OK"
                (let ([res (dispatch-command! cid cmd)])
                  (set! *tui-last-feedback* res)
                  res)))]
         [(equal? k "backspace")
          (let ([len (string-length *tui-input-buffer*)])
            (when (> len 0)
              (set! *tui-input-buffer* (substring *tui-input-buffer* 0 (- len 1))))
            "OK")]
         [(equal? k "space")
          (set! *tui-input-buffer* (string-append *tui-input-buffer* " "))
          "OK"]
         [(= (string-length k) 1)
          (set! *tui-input-buffer* (string-append *tui-input-buffer* k))
          "OK"]
         [else "OK"]))]

    ;; 2. Window resize & mouse events
    [(starts-with? msg-str "resize:") "OK"]
    [(starts-with? msg-str "mouse:") "OK"]

    ;; 3. Direct Text Command (TCP / WS / local)
    [else
     (let ([res (dispatch-command! cid msg-str)])
       (set! *tui-last-feedback* res)
       res)]))

;;; -----------------------------------------------------------------------------
;;; 10. Full-Screen Responsive ANSI Terminal Canvas (game-render)
;;; -----------------------------------------------------------------------------
(define (game-render)
  (let* ([sz (system-window-size)]
         [width (max 78 (car sz))]
         [height (max 22 (cadr sz))]
         [line-sep (string-append "+" (make-string (- width 2) #\-) "+")]
         
         ;; 1. Top Status Banner (Moon progress bar)
         [day-bar-len 10]
         [day-prog (/ (- *world-day-duration* *world-day-time-left*) *world-day-duration*)]
         [filled (min day-bar-len (max 0 (inexact->exact (round (* day-prog day-bar-len)))))]
         [empty (max 0 (- day-bar-len filled))]
         [time-bar (string-append "[" (make-string filled #\=) (make-string empty #\.) "]")]
         
         [stats-text
           (string-append "MOON " (number->string *world-moon*)
                          " | " time-bar " " (number->string (inexact->exact (round *world-day-time-left*))) "s"
                          " | Food: " (number->string (total-food-value)) "/" (number->string (count-villagers))
                          " | Gold: " (number->string *world-gold*) "G"
                          " | Cards: " (number->string (total-card-count)) "/" (number->string (calculate-card-limit))
                          " | [" *world-status* "]")]
         [stats-line (string-append "| " stats-text (make-string (max 0 (- width (+ (string-length stats-text) 3))) #\space) "|")]

         ;; 2. Board & Active Slots
         [board-title (string-append "+-- Board & Stacks " (make-string (max 0 (- width 22)) #\-) "+")]
         [stack-lines
           (if (null? *world-stacks*)
               (list (string-append "| (Empty board)" (make-string (max 0 (- width 17)) #\space) "|"))
               (map (lambda (s)
                      (let* ([id-num (stack-id s)]
                             [id-str (string-append "[" (if (< id-num 10) " " "") (number->string id-num) "] ")]
                             [cards (stack-cards s)]
                             [colored-cards (join-strings (map colorize-card cards) " ")]
                             [prog-str
                               (if (number? (stack-progress s))
                                   (let* ([pct (/ (stack-progress s) (stack-total-time s))]
                                          [f (min 8 (max 0 (inexact->exact (round (* pct 8)))))]
                                          [e (max 0 (- 8 f))]
                                          [rem-time (max 0.0 (- (stack-total-time s) (stack-progress s)))])
                                     (string-append " [" (make-string f #\=) (make-string e #\.) "] ("
                                                    (number->string (inexact->exact (round rem-time))) "s / "
                                                    (if (string? (stack-recipe-name s)) (stack-recipe-name s) "Harvest Berry") ")"))
                                   "")]
                             ;; For column spacing calculation, strip ANSI tags length
                             [raw-content (string-append id-str (join-strings cards " ") prog-str)]
                             [styled-content (string-append id-str colored-cards prog-str)])
                        (string-append "| " styled-content
                                       (make-string (max 0 (- width (+ (string-length raw-content) 3))) #\space) "|")))
                    *world-stacks*))]

         ;; 3. Marketplace Packs
         [shop-title (string-append "+-- Card Packs " (make-string (max 0 (- width 18)) #\-) "+")]
         [shop-line1 "| [b basic] Basic (3G)  |  [b food] Food (5G)  |  [b struct] Structures (10G)"]
         [shop-line2 "| [b explore] Exploration & Combat (12G)"]
         [p-line1 (string-append shop-line1 (make-string (max 0 (- width (string-length shop-line1) 1)) #\space) "|")]
         [p-line2 (string-append shop-line2 (make-string (max 0 (- width (string-length shop-line2) 1)) #\space) "|")]

         ;; 4. Idea & Recipe Hint Bar
         [tips-title (string-append "+-- Ideas & Crafting Guide " (make-string (max 0 (- width 29)) #\-) "+")]
         [tip-text "| • Spear: Villager+Wood+Flint  • Campfire: Villager+Wood(2)+Stone  • House: Wood(2)+Stone(2)"]
         [tips-line (string-append tip-text (make-string (max 0 (- width (string-length tip-text) 1)) #\space) "|")]

         ;; 5. Recent System Event Logs
         [logs-title (string-append "+-- Event Log & Chat " (make-string (max 0 (- width 24)) #\-) "+")]
         [recent-logs (let loop ([l *world-logs*] [n 4] [acc '()])
                        (if (or (null? l) (<= n 0))
                            (reverse acc)
                            (loop (cdr l) (- n 1) (cons (car l) acc))))]
         [log-lines (map (lambda (l)
                           (let ([content (string-append "• " l)])
                             (string-append "| " content (make-string (max 0 (- width (+ (string-length content) 3))) #\space) "|")))
                         recent-logs)]

         ;; 6. Command Bar Prompt & Hardware Cursor Positioning
         [prompt-title (string-append "+-- Command Prompt [" *tui-last-feedback* "] "
                                      (make-string (max 0 (- width (+ 24 (string-length *tui-last-feedback*)))) #\-) "+")]
         [cmd-content (string-append "Command: " *tui-input-buffer*)]
         [cmd-line (string-append "| " cmd-content (make-string (max 0 (- width (+ (string-length cmd-content) 3))) #\space) "|")]

         [grid (append
                 (list (string-append "+-- CardCraft Console " (make-string (max 0 (- width 24)) #\-) "+")
                       stats-line
                       board-title)
                 stack-lines
                 (list shop-title
                       p-line1
                       p-line2
                       tips-title
                       tips-line
                       logs-title)
                 log-lines
                 (list prompt-title
                       cmd-line
                       line-sep))]
         [cursor-x (+ 11 (string-length *tui-input-buffer*))]
         [cursor-y (- (length grid) 2)])

    (string-append
      "{"
        "\"cursor\": [" (number->string cursor-x) ", " (number->string cursor-y) "],"
        "\"grid\": ["
          (join-strings (map (lambda (r) (string-append "\"" (escape-json-str r) "\"")) grid) ",")
        "]"
      "}")))
