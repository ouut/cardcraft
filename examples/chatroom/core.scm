;;; =============================================================================
;;; CardCraft Example: Multi-User Terminal Chatroom (纯 Scheme 极简字符聊天室)
;;; =============================================================================
;;; 运行方式:
;;;   cargo run --release -- -g examples/chatroom
;;;
;;; 交互特性:
;;;   1. 本地控制台: 全屏 ASCII/ANSI 终端界面，直接打字聊天，按 Enter 发送。
;;;   2. 网页浏览器: 打开 http://localhost:8080 即时加入多人聊天。
;;;   3. TCP 终端:   通过 nc localhost 9000 接入。
;;;   4. P2P 穿透:   复制终端输出的 P2P Ticket 发给好友即可跨网联机。
;;; =============================================================================

;;; --- 全局状态 ---
(define *uptime* 0.0)
(define *net-clients* '())      ;; ((cid name) ...)
(define *chat-history* '())     ;; 最近消息列表 (从旧到新)
(define *max-history* 50)
(define *tui-input-buffer* "")
(define *tui-feedback* "Ready")

;;; --- ANSI 色彩常量 (VT100 规范) ---
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

;;; --- 基础字符串工具函数 ---
(define (starts-with? str prefix)
  (let ([plen (string-length prefix)]
        [slen (string-length str)])
    (and (>= slen plen)
         (equal? (substring str 0 plen) prefix))))

(define (split-spaces str)
  (let ([len (string-length str)])
    (let loop ([i 0] [curr '()] [tokens '()])
      (if (>= i len)
          (let ([last (list->string (reverse curr))])
            (reverse (if (equal? last "") tokens (cons last tokens))))
          (let ([ch (string-ref str i)])
            (if (char=? ch #\space)
                (let ([tok (list->string (reverse curr))])
                  (loop (+ i 1) '() (if (equal? tok "") tokens (cons tok tokens))))
                (loop (+ i 1) (cons ch curr) tokens)))))))

(define (join-strings lst sep)
  (cond
    [(null? lst) ""]
    [(null? (cdr lst)) (car lst)]
    [else
     (let loop ([rem (cdr lst)] [acc (car lst)])
       (if (null? rem)
           acc
           (loop (cdr rem) (string-append acc sep (car rem)))))]))

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

;;; --- 消息与客户端管理 ---
(define (add-message! msg)
  (let ([new-hist (append *chat-history* (list msg))])
    (if (> (length new-hist) *max-history*)
        (set! *chat-history* (cdr new-hist))
        (set! *chat-history* new-hist))))

(define (find-client cid)
  (let loop ([cs *net-clients*])
    (cond
      [(null? cs) #f]
      [(= (car (car cs)) cid) (car cs)]
      [else (loop (cdr cs))])))

(define (find-client-by-name name)
  (let loop ([cs *net-clients*])
    (cond
      [(null? cs) #f]
      [(equal? (cadr (car cs)) name) (car cs)]
      [else (loop (cdr cs))])))

(define (get-client-name cid)
  (let ([c (find-client cid)])
    (if c (cadr c) (string-append "Guest_" (number->string cid)))))

(define (update-client-name! cid new-name)
  (set! *net-clients*
        (map (lambda (c)
               (if (= (car c) cid)
                   (list cid new-name)
                   c))
             *net-clients*)))

(define (remove-client! cid)
  (set! *net-clients*
        (let loop ([cs *net-clients*] [acc '()])
          (cond
            [(null? cs) (reverse acc)]
            [(= (car (car cs)) cid) (loop (cdr cs) acc)]
            [else (loop (cdr cs) (cons (car cs) acc))]))))

;;; =============================================================================
;;; 游戏/应用生命周期钩子
;;; =============================================================================

(define (game-start!)
  (set! *uptime* 0.0)
  (set! *net-clients* (list (list 0 "Host")))
  (set! *chat-history* '())
  (add-message! (string-append ANSI-CYAN "*** Welcome to CardCraft Terminal Chatroom! ***" ANSI-RESET))
  (add-message! (string-append ANSI-GRAY "Type your message and press Enter. Commands: /help, /nick, /who, /w" ANSI-RESET))
  #t)

(define (game-update! dt)
  (set! *uptime* (+ *uptime* dt))
  #t)

;; 兼容 CAD 接口
(define (game-input . rest)
  (let* ([has-cid? (> (length rest) 1)]
         [cid (if has-cid? (car rest) 0)]
         [tokens (if has-cid? (cadr rest) (car rest))])
    (dispatch-chat! cid (join-strings tokens " "))))

;;; =============================================================================
;;; 网络事件处理与业务路由 (纯 Scheme 路由与广播分发)
;;; =============================================================================

;; 纯 Scheme 实现全员广播：向当前所有在线客户端分别执行纯网络下发
(define (broadcast! msg)
  (for-each (lambda (c) (system-net-send! (car c) msg)) *net-clients*))

;; 钩子 1: 新客户端连入
(define (net-on-connect! cid)
  (if (find-client cid)
      "OK"
      (let ([def-name (if (= cid 0) "Host" (string-append "Guest_" (number->string cid)))])
        (set! *net-clients* (append *net-clients* (list (list cid def-name))))
        (when (> cid 0)
          (system-net-send! cid (game-render))
          (let ([join-msg (string-append ANSI-CYAN "*** " def-name " joined the chatroom ***" ANSI-RESET)])
            (broadcast! join-msg)
            (add-message! join-msg))
          (system-net-send! cid (string-append ANSI-GREEN "=== Welcome to CardCraft Chatroom, " def-name "! ===" ANSI-RESET))
          (system-net-send! cid (string-append ANSI-GRAY "Commands: /nick <name>, /w <user> <msg>, /who, /help" ANSI-RESET)))
        "OK")))

;; 钩子 2: 客户端断开
(define (net-on-disconnect! cid)
  (let ([name (get-client-name cid)])
    (remove-client! cid)
    (when (> cid 0)
      (let ([leave-msg (string-append ANSI-GRAY "*** " name " left the chatroom ***" ANSI-RESET)])
        (broadcast! leave-msg)
        (add-message! leave-msg)))
    "OK"))

;; 钩子 3: 消息到达 (包括键盘直通与网络包)
(define (net-on-message! cid msg-str)
  (cond
    ;; 本地键盘事件
    [(starts-with? msg-str "key:")
     (let ([k (substring msg-str 4 (string-length msg-str))])
       (cond
         [(equal? k "enter")
          (let ([cmd *tui-input-buffer*])
            (set! *tui-input-buffer* "")
            (if (equal? cmd "")
                "OK"
                (let ([res (dispatch-chat! cid cmd)])
                  (set! *tui-feedback* res)
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

    ;; 窗口尺寸变动与鼠标事件
    [(starts-with? msg-str "resize:") "OK"]
    [(starts-with? msg-str "mouse:") "OK"]

    ;; 客户端纯文本消息
    [else
     (let ([res (dispatch-chat! cid msg-str)])
       (set! *tui-feedback* res)
       res)]))

;;; --- 聊天业务分发逻辑 ---
(define (dispatch-chat! cid text)
  (let* ([sender (get-client-name cid)]
         [is-slash? (starts-with? text "/")]
         [raw-line (if is-slash?
                       (substring text 1 (string-length text))
                       text)]
         [tokens (split-spaces raw-line)])
    (if (null? tokens)
        "Empty message"
        (let ([cmd (car tokens)]
              [args (cdr tokens)])
          (cond
            ;; 1. 改名: /nick <new-name> or nick <new-name>
            [(and (or is-slash? (equal? cmd "nick")) (equal? cmd "nick"))
             (if (null? args)
                 (begin
                   (system-net-send! cid (string-append ANSI-RED "Usage: /nick <new-name>" ANSI-RESET))
                   "Usage: /nick <new-name>")
                 (let ([new-name (car args)])
                   (if (find-client-by-name new-name)
                       (begin
                         (system-net-send! cid (string-append ANSI-RED "Name '" new-name "' is already taken." ANSI-RESET))
                         "Name taken")
                       (begin
                         (update-client-name! cid new-name)
                         (let ([notice (string-append ANSI-CYAN "*** " sender " is now known as " new-name " ***" ANSI-RESET)])
                           (broadcast! notice)
                           (add-message! notice))
                         "OK"))))]

            ;; 2. 查在线成员: /who or /users or /online
            [(and (or is-slash? (null? args)) (member cmd '("who" "users" "online")))
             (let* ([names (map (lambda (c) (cadr c)) *net-clients*)]
                    [msg (string-append ANSI-CYAN "Online users (" (number->string (length names)) "): " ANSI-BOLD (join-strings names ", ") ANSI-RESET)])
               (system-net-send! cid msg)
               (when (= cid 0) (add-message! msg))
               "OK")]

            ;; 3. 私聊: /w <user> <message...> or /tell <user> <message...> or /whisper <user> <message...>
            [(and (or is-slash? (and (member cmd '("w" "tell" "whisper")) (not (null? args)) (find-client-by-name (car args))))
                  (member cmd '("w" "tell" "whisper")))
             (if (< (length args) 2)
                 (begin
                   (system-net-send! cid (string-append ANSI-RED "Usage: /w <user> <message...>" ANSI-RESET))
                   "Usage: /w <user> <message...>")
                 (let* ([target-name (car args)]
                        [chat-body (join-strings (cdr args) " ")]
                        [target (find-client-by-name target-name)])
                   (if (not target)
                       (begin
                         (system-net-send! cid (string-append ANSI-RED "User '" target-name "' not found." ANSI-RESET))
                         "User not found")
                       (let ([target-cid (car target)]
                             [to-msg (string-append ANSI-MAGENTA "[Private from " sender "]: " chat-body ANSI-RESET)]
                             [from-msg (string-append ANSI-MAGENTA "[Private to " target-name "]: " chat-body ANSI-RESET)])
                         (system-net-send! target-cid to-msg)
                         (system-net-send! cid from-msg)
                         (when (= target-cid 0) (add-message! to-msg))
                         (when (= cid 0) (add-message! from-msg))
                         "OK"))))]

            ;; 4. 清屏: /clear
            [(and (or is-slash? (null? args)) (equal? cmd "clear"))
             (when (= cid 0) (set! *chat-history* '()))
             (system-net-send! cid "Cleared")
             "OK"]

            ;; 5. 帮助: /help
            [(and (or is-slash? (null? args)) (equal? cmd "help"))
             (let ([help-text (string-append
                               ANSI-YELLOW "Chatroom Commands:\n"
                               "  /nick <name>        - Change your nickname\n"
                               "  /w <user> <msg>     - Send a private message\n"
                               "  /who                - View online users\n"
                               "  /clear              - Clear message screen\n"
                               "  /help               - Show this help menu" ANSI-RESET)])
               (system-net-send! cid help-text)
               (when (= cid 0) (add-message! help-text))
               "OK")]

            ;; 6. 默认公共公屏聊天 (带 ANSI 彩色高亮昵称)
            [else
             (let ([pub-msg (string-append ANSI-YELLOW "<" sender "> " ANSI-RESET text)])
               (broadcast! pub-msg)
               (add-message! pub-msg)
               "OK")])))))

;;; =============================================================================
;;; 全屏 100% Scheme 渲染导出 (game-render)
;;; =============================================================================

(define (game-render)
  (let* ([sz (system-window-size)]
         [width (max 70 (car sz))]
         [height (max 18 (cadr sz))]
         [line-sep (string-append "+" (make-string (- width 2) #\-) "+")]
         
         ;; 1. 顶栏信息
         [online-count (length *net-clients*)]
         [uptime-sec (round *uptime*)]
         [header-info (string-append "💬 CardCraft Chatroom | Online: " (number->string online-count)
                                     " | Uptime: " (number->string (inexact->exact uptime-sec)) "s")]
         [header-title (string-append "+-- " header-info " "
                                      (make-string (max 0 (- width (+ (string-length header-info) 5))) #\-) "+")]
         [tips-text "| Press Enter to send, '/help' for commands, '/nick <name>' to rename."]
         [tips-line (string-append tips-text (make-string (max 0 (- width (string-length tips-text) 1)) #\space) "|")]

         ;; 2. 在线列表行
         [users-title (string-append "+-- Online Users (" (number->string online-count) ") "
                                     (make-string (max 0 (- width (+ (string-length (number->string online-count)) 21))) #\-) "+")]
         [names (map (lambda (c) (string-append "• " (cadr c))) *net-clients*)]
         [names-joined (join-strings names "  ")]
         [users-text (string-append "| " names-joined)]
         [users-line (string-append users-text (make-string (max 0 (- width (string-length users-text) 1)) #\space) "|")]

         ;; 3. 消息列表区 (自适应填充剩余行高)
         [msgs-title (string-append "+-- Chat Messages (" (number->string (length *chat-history*)) ") "
                                    (make-string (max 0 (- width (+ (string-length (number->string (length *chat-history*))) 23))) #\-) "+")]
         ;; 预留: Header(2) + Users(2) + MessagesTitle(1) + Input(3) = 8 行
         [max-msg-lines (max 4 (- height 9))]
         [visible-msgs
           (let ([total (length *chat-history*)])
             (if (<= total max-msg-lines)
                 *chat-history*
                 (let loop ([rem *chat-history*] [skip (- total max-msg-lines)])
                   (if (<= skip 0)
                       rem
                       (loop (cdr rem) (- skip 1))))))]
         [msg-lines
           (map (lambda (m)
                  (let ([content (string-append "| " m)])
                    ;; 为右边框预留空间
                    (string-append content (make-string (max 0 (- width (string-length content) 1)) #\space) "|")))
                visible-msgs)]

         ;; 4. 输入栏与反馈
         [prompt-title (string-append "+-- Send Message [" *tui-feedback* "] "
                                      (make-string (max 0 (- width (+ (string-length *tui-feedback*) 21))) #\-) "+")]
         [input-content (string-append "| > " *tui-input-buffer*)]
         [input-line (string-append input-content (make-string (max 0 (- width (string-length input-content) 1)) #\space) "|")]

         ;; 组合全部网格行
         [grid (append
                 (list header-title
                       tips-line
                       users-title
                       users-line
                       msgs-title)
                 msg-lines
                 (list prompt-title
                       input-line
                       line-sep))]
         ;; 光标计算
         [cursor-x (+ 4 (string-length *tui-input-buffer*))]
         [cursor-y (- (length grid) 2)])

    (string-append
      "{"
        "\"cursor\": [" (number->string cursor-x) ", " (number->string cursor-y) "],"
        "\"grid\": ["
          (join-strings (map (lambda (r) (string-append "\"" (escape-json-str r) "\"")) grid) ",")
        "]"
      "}")))
