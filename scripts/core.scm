;;; =============================================================================
;;; CardCraft Core Game Engine (Written in Steel Scheme)
;;; =============================================================================

;; Global World State
;; world: (meta stacks logs recipes shop-items)
(define *world-moon* 1)
(define *world-day-time-left* 30.0)
(define *world-day-duration* 30.0)
(define *world-gold* 5)
(define *world-status* "RUNNING")
(define *world-stacks* '())
(define *world-logs* '("Welcome to CardCraft! Type 'h' for commands."))
(define *next-stack-id* 1)

;; Helper: Join list of strings with separator
(define (join-strings lst sep)
  (if (null? lst)
      ""
      (if (null? (cdr lst))
          (car lst)
          (string-append (car lst) sep (join-strings (cdr lst) sep)))))

;; Helper: Split string by whitespace into list of words
(define (split-spaces str)
  (let ([len (string-length str)])
    (let loop ([i 0] [cur '()] [words '()])
      (if (>= i len)
          (if (null? cur)
              (reverse words)
              (reverse (cons (list->string (reverse cur)) words)))
          (let ([ch (string-ref str i)])
            (if (or (char=? ch #\space) (char=? ch #\tab) (char=? ch #\newline))
                (if (null? cur)
                    (loop (+ i 1) '() words)
                    (loop (+ i 1) '() (cons (list->string (reverse cur)) words)))
                (loop (+ i 1) (cons ch cur) words)))))))

;; Helper: Multiset operations
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

;; Helper: Pick random item from list using Rust RNG bridge
(define (random-choice lst)
  (if (null? lst)
      #f
      (let* ([len (length lst)]
             [idx (system-utils-random-int 0 (- len 1))])
        (list-ref lst idx))))

;; Card Value Lookups
(define (card-food-value card)
  (cond
    [(equal? card "Berry") 1]
    [(equal? card "Apple") 1]
    [(equal? card "CookedBerry") 2]
    [else 0]))

(define (card-sell-value card)
  (cond
    [(equal? card "Coin") 1]
    [(equal? card "Berry") 1]
    [(equal? card "Wood") 1]
    [(equal? card "Stone") 1]
    [(equal? card "Apple") 1]
    [(equal? card "CookedBerry") 2]
    [(equal? card "Campfire") 3]
    [(equal? card "House") 5]
    [(equal? card "Shed") 4]
    [(equal? card "BerryBush") 2]
    [(equal? card "Tree") 2]
    [(equal? card "Rock") 2]
    [else 0]))

;; Recipe Table: (name inputs duration outputs consumed)
(define *recipes*
  '(("Harvest Berry" ("Villager" "BerryBush") 4.0 ("Berry") ())
    ("Chop Tree" ("Villager" "Tree") 5.0 ("Wood") ())
    ("Mine Rock" ("Villager" "Rock") 5.0 ("Stone") ())
    ("Cook Berry" ("Campfire" "Berry") 3.0 ("CookedBerry") ("Berry"))
    ("Build Campfire" ("Villager" "Wood" "Wood" "Stone") 6.0 ("Campfire") ("Wood" "Wood" "Stone"))
    ("Build Shed" ("Villager" "Wood" "Stone" "Stone") 6.0 ("Shed") ("Wood" "Stone" "Stone"))
    ("Build House" ("Villager" "Wood" "Wood" "Stone" "Stone") 8.0 ("House") ("Wood" "Wood" "Stone" "Stone"))
    ("Breed Villager" ("Villager" "Villager" "House") 10.0 ("Baby") ())
    ("Grow Baby" ("Baby") 10.0 ("Villager") ("Baby"))))

;; Matching recipe
(define (find-matching-recipe cards)
  (define (loop r-list)
    (if (null? r-list)
        #f
        (let* ([r (car r-list)]
               [inputs (cadr r)])
          (if (multiset-subset? inputs cards)
              r
              (loop (cdr r-list))))))
  (loop *recipes*))

;; Stack accessors
;; A stack is: (id cards progress total-time recipe-name status)
(define (make-stack id cards progress total-time recipe-name status)
  (list id cards progress total-time recipe-name status))

(define (stack-id s) (car s))
(define (stack-cards s) (cadr s))
(define (stack-progress s) (caddr s))
(define (stack-total-time s) (cadddr s))
(define (stack-recipe-name s) (list-ref s 4))
(define (stack-status s) (list-ref s 5))

(define (add-log! msg)
  (set! *world-logs* (cons msg *world-logs*)))

;; Re-evaluate stack recipe
(define (refresh-stack-recipe s)
  (let* ([id (stack-id s)]
         [cards (stack-cards s)]
         [rec (find-matching-recipe cards)])
    (if rec
        (let ([name (car rec)]
              [dur (caddr rec)])
          ;; If recipe is same, preserve progress unless progress > dur
          (if (and (equal? (stack-recipe-name s) name)
                   (number? (stack-progress s)))
              s
              (make-stack id cards 0.0 dur name "Crafting")))
        (make-stack id cards '() '() '() "Idle"))))

;; Calculate total card count
(define (total-card-count)
  (define (loop stks acc)
    (if (null? stks)
        acc
        (loop (cdr stks) (+ acc (length (stack-cards (car stks)))))))
  (loop *world-stacks* 0))

;; Calculate card limit (15 base + 5 per Shed)
(define (calculate-card-limit)
  (define (count-sheds stks acc)
    (if (null? stks)
        acc
        (let ([cards (stack-cards (car stks))])
          (define (count-in-cards c-list sub-acc)
            (cond
              [(null? c-list) sub-acc]
              [(equal? (car c-list) "Shed") (count-in-cards (cdr c-list) (+ sub-acc 1))]
              [else (count-in-cards (cdr c-list) sub-acc)]))
          (count-sheds (cdr stks) (count-in-cards cards acc)))))
  (+ 15 (* 5 (count-sheds *world-stacks* 0))))

;; Count villagers
(define (count-villagers)
  (define (loop stks acc)
    (if (null? stks)
        acc
        (let ([cards (stack-cards (car stks))])
          (define (count-in-cards c-list sub-acc)
            (cond
              [(null? c-list) sub-acc]
              [(equal? (car c-list) "Villager") (count-in-cards (cdr c-list) (+ sub-acc 1))]
              [else (count-in-cards (cdr c-list) sub-acc)]))
          (loop (cdr stks) (count-in-cards cards acc)))))
  (loop *world-stacks* 0))

;; Count total food value on board
(define (total-food-value)
  (define (loop stks acc)
    (if (null? stks)
        acc
        (let ([cards (stack-cards (car stks))])
          (define (food-in-cards c-list sub-acc)
            (if (null? c-list)
                sub-acc
                (food-in-cards (cdr c-list) (+ sub-acc (card-food-value (car c-list))))))
          (loop (cdr stks) (food-in-cards cards acc)))))
  (loop *world-stacks* 0))

;; Allocate new stack slot
(define (get-next-id!)
  (let ([id *next-stack-id*])
    (set! *next-stack-id* (+ *next-stack-id* 1))
    id))

;; Spawn card to empty/new stack
(define (spawn-card! card)
  (let ([id (get-next-id!)])
    (let ([new-s (refresh-stack-recipe (make-stack id (list card) '() '() '() "Idle"))])
      (set! *world-stacks* (append *world-stacks* (list new-s))))))

;; Find stack by ID
(define (find-stack id)
  (define (loop stks)
    (cond
      [(null? stks) #f]
      [(= (stack-id (car stks)) id) (car stks)]
      [else (loop (cdr stks))]))
  (loop *world-stacks*))

;; Remove stack by ID
(define (remove-stack id)
  (define (loop stks)
    (cond
      [(null? stks) '()]
      [(= (stack-id (car stks)) id) (cdr stks)]
      [else (cons (car stks) (loop (cdr stks)))]))
  (set! *world-stacks* (loop *world-stacks*)))

;; Replace stack by ID
(define (update-stack! s)
  (define (loop stks)
    (cond
      [(null? stks) '()]
      [(= (stack-id (car stks)) (stack-id s)) (cons s (cdr stks))]
      [else (cons (car stks) (loop (cdr stks)))]))
  (set! *world-stacks* (loop *world-stacks*)))

;; Initialize Game
(define (game-start!)
  (set! *world-moon* 1)
  (set! *world-day-time-left* 30.0)
  (set! *world-day-duration* 30.0)
  (set! *world-gold* 5)
  (set! *world-status* "RUNNING")
  (set! *next-stack-id* 1)
  (set! *world-logs* '("Game started! Stack Villager onto BerryBush (s 1 2)."))
  (set! *world-stacks* '())
  ;; Starting board:
  (spawn-card! "Villager")   ; 1
  (spawn-card! "BerryBush")  ; 2
  (spawn-card! "Berry")      ; 3
  (spawn-card! "Wood")       ; 4
  (spawn-card! "Coin")       ; 5
  "Initialized")

;; Advance time tick
(define (game-update! dt)
  (if (equal? *world-status* "RUNNING")
      (begin
        ;; 1. Update stack progress
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
                            ;; Recipe complete!
                            (let* ([rec (find-matching-recipe (stack-cards s))]
                                   [rec-outputs (list-ref rec 3)]
                                   [rec-consumed (list-ref rec 4)]
                                   [remaining (multiset-diff (stack-cards s) rec-consumed)])
                              (add-log! (string-append "Produced: " (join-strings rec-outputs ", ")))
                              (set! pending-spawns (append pending-spawns rec-outputs))
                              (if (null? remaining)
                                  (tick-stacks (cdr stks)) ; stack emptied
                                  (let ([refreshed (refresh-stack-recipe (make-stack (stack-id s) remaining '() '() '() "Idle"))])
                                    (cons refreshed (tick-stacks (cdr stks))))))
                            ;; Still progressing
                            (cons (make-stack (stack-id s) (stack-cards s) new-prog total (stack-recipe-name s) "Crafting")
                                  (tick-stacks (cdr stks)))))
                      ;; Idle stack
                      (cons s (tick-stacks (cdr stks)))))))
          (set! *world-stacks* (tick-stacks *world-stacks*))
          (for-each spawn-card! pending-spawns))

        ;; 2. Day timer progression
        (set! *world-day-time-left* (- *world-day-time-left* dt))
        (if (<= *world-day-time-left* 0.0)
            ;; Moon End Cycle
            (begin
              (let* ([req (count-villagers)]
                     [available (total-food-value)])
                (if (< available req)
                    (add-log! "WARNING: Not enough food! A villager starved!")
                    (begin
                      (add-log! (string-append "Moon ended! Consumed " (number->string req) " food."))
                      ;; Deduct food
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
                        (set! *world-stacks* (consume-from-stacks *world-stacks*)))))
                (set! *world-moon* (+ *world-moon* 1))
                (set! *world-day-time-left* *world-day-duration*)
                (add-log! (string-append "=== Moon " (number->string *world-moon*) " Begun! ===")))))
      #f)))

;; CAD Command Handlers (supports both (game-input tokens) and (game-input cid tokens))
(define (game-input . rest)
  (let* ([has-cid? (> (length rest) 1)]
         [cid (if has-cid? (car rest) #f)]
         [tokens (if has-cid? (cadr rest) (car rest))])
    (if (null? tokens)
        "Empty command"
        (let ([verb (car tokens)]
              [args (cdr tokens)])
          (cond
          ;; Stack: s <from> <to>
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
                      (let* ([merged-cards (append (stack-cards s-to) (stack-cards s-from))]
                             [new-to (refresh-stack-recipe (make-stack to-id merged-cards '() '() '() "Idle"))])
                        (remove-stack from-id)
                        (update-stack! new-to)
                        (add-log! (string-append "Stacked [" (car args) "] onto [" (cadr args) "]."))
                        "OK")]))))]

          ;; Unstack: u <id>
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

          ;; Move: m <from> <to>
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

          ;; Buy Pack: b <pack-name>
          [(or (equal? verb "b") (equal? verb "buy"))
           (if (< (length args) 1)
               "Usage: b <basic|food>"
               (let ([pack (car args)])
                 (cond
                   [(equal? pack "basic")
                    (if (< *world-gold* 3)
                        "Not enough gold! Basic Pack costs 3G."
                        (begin
                          (set! *world-gold* (- *world-gold* 3))
                          (spawn-card! "Pack:Basic")
                          (add-log! "Purchased Basic Pack (3G).")
                          "OK"))]
                   [(equal? pack "food")
                    (if (< *world-gold* 5)
                        "Not enough gold! Food Pack costs 5G."
                        (begin
                          (set! *world-gold* (- *world-gold* 5))
                          (spawn-card! "Pack:Food")
                          (add-log! "Purchased Food Pack (5G).")
                          "OK"))]
                   [else "Unknown pack. Available: basic, food"])))]

          ;; Open Pack: o <slot-id>
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
                              (let* ([pool '("Wood" "Stone" "BerryBush" "Berry" "Coin")]
                                     [c1 (random-choice pool)]
                                     [c2 (random-choice pool)]
                                     [c3 (random-choice pool)])
                                (spawn-card! c1)
                                (spawn-card! c2)
                                (spawn-card! c3)
                                (add-log! (string-append "Opened Basic Pack! Got " c1 ", " c2 ", " c3 "."))
                                "OK"))]
                           [(member "Pack:Food" cards)
                            (let ([rem (remove-one "Pack:Food" cards)])
                              (if (null? rem)
                                  (remove-stack id)
                                  (update-stack! (refresh-stack-recipe (make-stack id rem '() '() '() "Idle"))))
                              (let* ([pool '("Berry" "Berry" "Apple" "BerryBush")]
                                     [c1 (random-choice pool)]
                                     [c2 (random-choice pool)]
                                     [c3 (random-choice pool)])
                                (spawn-card! c1)
                                (spawn-card! c2)
                                (spawn-card! c3)
                                (add-log! (string-append "Opened Food Pack! Got " c1 ", " c2 ", " c3 "."))
                                "OK"))]
                           [else "No unopened pack in this stack."]))))))]

          ;; Sell: x <slot-id>
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

          ;; Wait: w [seconds]
          [(or (equal? verb "w") (equal? verb "wait"))
           (let ([dt (if (null? args) 1.0 (string->number (car args)))])
             (game-update! dt)
             (add-log! (string-append "Waited " (number->string dt) "s."))
             "OK")]

          ;; Pause: p
          [(or (equal? verb "p") (equal? verb "pause"))
           (if (equal? *world-status* "RUNNING")
               (begin (set! *world-status* "PAUSED") (add-log! "Game paused.") "PAUSED")
               (begin (set! *world-status* "RUNNING") (add-log! "Game resumed.") "RUNNING"))]

          ;; Help: h
          [(or (equal? verb "h") (equal? verb "help") (equal? verb "?"))
           (add-log! "Commands: s <from> <to> | u <id> | m <from> <to> | b <pack> | o <id> | x <id> | w [s] | p")
           "OK"]

          [else
           (string-append "Unknown CAD command: " verb ". Type 'h' for help.")])))))

;; JSON Serializer for Game View Snapshot
(define (to-json-str v)
  (cond
    [(number? v) (number->string v)]
    [(boolean? v) (if v "true" "false")]
    [(string? v) (string-append "\"" v "\"")]
    [(symbol? v) (string-append "\"" (symbol->string v) "\"")]
    [(null? v) "null"]
    [(and (pair? v) (equal? (car v) 'object))
     (string-append "{"
                    (join-strings
                      (map (lambda (pair)
                             (string-append "\"" (symbol->string (car pair)) "\":"
                                            (to-json-str (cadr pair))))
                           (cadr v))
                      ",")
                    "}")]
    [(list? v)
     (string-append "[" (join-strings (map to-json-str v) ",") "]")]
    [else "\"unknown\""]))

(define *tui-input-buffer* "")
(define *tui-last-feedback* "OK")

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
              [else (loop (+ i 1) (cons ch acc))]))))))

;; Export full-screen character canvas as JSON
(define (game-render)
  (let* ([sz (system-window-size)]
         [width (max 80 (car sz))]
         [line-sep (string-append "+" (make-string (- width 2) #\-) "+")]
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
         
         [board-title (string-append "+-- Board & Stacks " (make-string (max 0 (- width 22)) #\-) "+")]
         [stack-lines
           (if (null? *world-stacks*)
               (list (string-append "| (Empty board)" (make-string (max 0 (- width 17)) #\space) "|"))
               (map (lambda (s)
                      (let* ([id-num (stack-id s)]
                             [id-str (string-append "[" (if (< id-num 10) " " "") (number->string id-num) "] ")]
                             [cards-str (join-strings (stack-cards s) " ")]
                             [prog-str (if (number? (stack-progress s))
                                           (let* ([pct (/ (stack-progress s) (stack-total-time s))]
                                                  [f (min 8 (max 0 (inexact->exact (round (* pct 8)))))]
                                                  [e (max 0 (- 8 f))]
                                                  [rem-time (max 0.0 (- (stack-total-time s) (stack-progress s)))])
                                             (string-append " [" (make-string f #\=) (make-string e #\.) "] ("
                                                            (number->string (inexact->exact (round rem-time))) "s / "
                                                            (if (string? (stack-recipe-name s)) (stack-recipe-name s) "Harvest Berry") ")"))
                                           "")]
                             [content (string-append id-str cards-str prog-str)])
                        (string-append "| " content (make-string (max 0 (- width (+ (string-length content) 3))) #\space) "|")))
                    *world-stacks*))]
         
         [actions-title (string-append "+-- Actions & Store " (make-string (max 0 (- width 23)) #\-) "+")]
         [act-text "ACTIONS: [b basic] Basic Pack (3G)  |  [b food] Food Pack (5G)"]
         [actions-line (string-append "| " act-text (make-string (max 0 (- width (+ (string-length act-text) 3))) #\space) "|")]
         
         [logs-title (string-append "+-- System Event Log & Chat " (make-string (max 0 (- width 30)) #\-) "+")]
         [recent-logs (let loop ([l *world-logs*] [n 4] [acc '()])
                        (if (or (null? l) (<= n 0))
                            (reverse acc)
                            (loop (cdr l) (- n 1) (cons (car l) acc))))]
         [log-lines (map (lambda (l)
                           (let ([content (string-append "• " l)])
                             (string-append "| " content (make-string (max 0 (- width (+ (string-length content) 3))) #\space) "|")))
                         recent-logs)]
         
         [prompt-title (string-append "+-- Command Prompt [" *tui-last-feedback* "] " (make-string (max 0 (- width (+ 24 (string-length *tui-last-feedback*)))) #\-) "+")]
         [cmd-content (string-append "Command: " *tui-input-buffer*)]
         [cmd-line (string-append "| " cmd-content (make-string (max 0 (- width (+ (string-length cmd-content) 3))) #\space) "|")]
         
         [grid (append
                 (list (string-append "+-- CardCraft Console " (make-string (max 0 (- width 24)) #\-) "+")
                       stats-line
                       board-title)
                 stack-lines
                 (list actions-title
                       actions-line
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

;;; =============================================================================
;;; Network Hooks, Online State & Chat Dispatcher (The 3 Hooks)
;;; =============================================================================

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
        (map (lambda (item)
               (if (equal? (car item) cid)
                   (list (car item) new-name (caddr item))
                   item))
             *net-clients*)))

(define (remove-net-client! cid)
  (set! *net-clients*
        (filter (lambda (item) (not (equal? (car item) cid))) *net-clients*)))

;; Hook 1: net-on-connect!
(define (net-on-connect! cid)
  (let ([default-name (string-append "Player_" (number->string cid))])
    (set! *net-clients* (cons (list cid default-name "Lobby") *net-clients*))
    (system-net-send! cid (string-append "=== Welcome to CardCraft Online! You are " default-name " ==="))
    (system-net-broadcast! (string-append ">>> " default-name " connected to the server."))
    "OK"))

;; Hook 2: net-on-disconnect!
(define (net-on-disconnect! cid)
  (let ([c (find-net-client cid)])
    (if c
        (let ([name (cadr c)])
          (remove-net-client! cid)
          (system-net-broadcast! (string-append "<<< " name " disconnected from the server."))
          "OK")
        "OK")))

(define (starts-with? str prefix)
  (let ([plen (string-length prefix)]
        [slen (string-length str)])
    (and (>= slen plen)
         (equal? (substring str 0 plen) prefix))))

(define (dispatch-command! cid msg-str)
  (let* ([c (find-net-client cid)]
         [name (if c (cadr c) (string-append "Player_" (number->string cid)))]
         [tokens (split-spaces msg-str)])
    (if (null? tokens)
        "Empty command"
        (let ([verb (car tokens)]
              [args (cdr tokens)])
          (cond
            ;; 1. Nickname: nick <new-name>
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
                         (system-net-broadcast! (string-append "*** " name " is now known as " new-name "."))
                         "OK"))))]

            ;; 2. Public Chat: say <text...>
            [(equal? verb "say")
             (if (null? args)
                 (begin
                   (system-net-send! cid "Usage: say <message...>")
                   "Usage: say <message...>")
                 (let ([chat-text (join-strings args " ")])
                   (system-net-broadcast! (string-append "[" name "]: " chat-text))
                   (add-log! (string-append "[" name "]: " chat-text))
                   "OK"))]

            ;; 3. Whisper / Private Message: w <target> <text...> or tell <target> <text...>
            [(or (equal? verb "w") (equal? verb "tell") (equal? verb "whisper"))
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

            ;; 4. Who / Users list: who or users
            [(or (equal? verb "who") (equal? verb "users") (equal? verb "online"))
             (let* ([names (map (lambda (cl) (cadr cl)) *net-clients*)]
                    [msg (string-append "Online users (" (number->string (length names)) "): " (join-strings names ", "))])
               (system-net-send! cid msg)
               msg)]

            ;; 5. Otherwise: CAD Game Command (s 1 2, b 1, u 1, etc.)
            [else
             (let ([res (game-input cid tokens)])
               (system-net-send! cid (string-append "Result: " res))
               res)])))))

;; Hook 3: net-on-message!
(define (net-on-message! cid msg-str)
  (cond
    ;; 1. TUI key events from keyboard
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
            (if (> len 0)
                (set! *tui-input-buffer* (substring *tui-input-buffer* 0 (- len 1)))
                #f)
            "OK")]
         [(equal? k "space")
          (set! *tui-input-buffer* (string-append *tui-input-buffer* " "))
          "OK"]
         [(and (= (string-length k) 1))
          (set! *tui-input-buffer* (string-append *tui-input-buffer* k))
          "OK"]
         [else "OK"]))]

    ;; 2. Window resize event
    [(starts-with? msg-str "resize:")
     "OK"]

    ;; 3. Mouse event
    [(starts-with? msg-str "mouse:")
     "OK"]

    ;; 4. Direct text command (from TCP, WebSocket, or local test)
    [else
     (let ([res (dispatch-command! cid msg-str)])
       (set! *tui-last-feedback* res)
       res)]))
