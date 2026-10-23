# CardCraft AI Agent API Reference & Engine Specification

> **Target Audience**: AI Coding Agents / LLMs.  
> **Purpose**: Exact, code-verified specification of the CardCraft Rust host and Steel Scheme guest interface. Every signature, event format, and JSON schema below matches the actual implementation in `src/`.

---

## 1. Architecture & File Map

```text
src/
├── main.rs      # CLI parser, Tokio LocalSet runtime, Crossterm TUI loop (client_id = 0)
├── engine.rs    # Steel Scheme VM wrapper, system-* primitive registration, sandboxed I/O
├── server.rs    # EngineActor, ClientHub (per-cid mpsc router), TCP (:9000) & HTTP/WS (:8080)
├── p2p.rs       # Iroh QUIC P2P host listener & client bridge actor
├── view.rs      # GameViewSnapshot { grid: Vec<String>, cursor: Option<[u16; 2]> }
├── render.rs    # ANSI escape parser -> Ratatui Frame renderer + hardware cursor placement
└── buffer.rs    # VirtualScreen (headless Ratatui TestBackend for automated assertions)
scripts/
└── core.scm     # Default game (Stacklands recreation) embedded via include_str!
examples/
└── chatroom/    # Standalone multi-user chatroom app (-g examples/chatroom)
```

### Core Invariants
1. **Zero Game State in Rust**: Rust never stores cards, maps, rooms, or player states.
2. **Pure `system-net-send!` Network Pipe**: Rust's TCP, WebSocket, and P2P servers **never** call `get_snapshot()` or wrap messages in custom JSON envelopes. Every byte sent to a network client (`cid > 0`) comes strictly from Scheme calling `(system-net-send! cid msg)`.
3. **Single-Threaded Actor Execution**: `GameEngine` (`!Send`) runs inside a `tokio::task::spawn_local` actor loop (`src/server.rs`). All local TUI, TCP, WS, P2P, and 100ms tick events are serialized through `EngineCommand`.

---

## 2. Game Directory Resolution & Sandbox (`src/engine.rs`)

When initialized via `GameEngine::with_game_dir(game_dir)` (CLI `-g <DIR>`, default `.`):
1. **Script Loading Priority**:
   - `<game_dir>/core.scm`
   - `<game_dir>/main.scm`
   - Embedded fallback `include_str!("../scripts/core.scm")`
2. **Startup Call**: Rust immediately evaluates the script and then invokes `(game-start!)`.
3. **File Sandbox (`<game_dir>/saves/`)**:
   - `resolve_sandboxed_path` canonicalizes paths and enforces `canonical_target.starts_with(&canonical_sandbox)`.
   - Any path traversal (`../`) returns `false` (on write) or `""` (on read).
4. **Web Directory Resolution (`src/main.rs`)**:
   - `--serve <DIR>` / `--web-dir <DIR>` if specified; otherwise `<game_dir>/web` if it exists; otherwise `./web`.

---

## 3. Host Primitives Registered in Scheme (`src/engine.rs`)

| Scheme Function | Alias (Deprecated) | Signature (Rust -> Scheme) | Behavior |
| :--- | :--- | :--- | :--- |
| `(system-log level msg)` | `(rust-log level msg)` | `(String, String) -> void` | Prints `[{level}] {msg}` to `stderr`. |
| `(system-utils-random-int min max)` | `(rust-random-int min max)` | `(i64, i64) -> i64` | Inclusive random integer `[min, max]`. Returns `min` if `min >= max`. |
| `(system-window-size)` | - | `() -> '(isize isize)` | Returns 2-element list `(width height)` from `crossterm::terminal::size()`, fallback `'(80 25)`. Access width via `(car (system-window-size))`, height via `(cadr (system-window-size))`. |
| `(system-time-now)` | - | `() -> f64` | Unix epoch timestamp in seconds as `f64`. |
| - | `(rust-millis)` | `() -> i64` | Unix epoch timestamp in milliseconds as `i64`. |
| `(system-file-write path content)` | - | `(String, String) -> bool` | Writes `content` to `<game_dir>/saves/<path>`. Returns `#true` or `#false`. |
| `(system-file-read path)` | - | `(String) -> String` | Reads `<game_dir>/saves/<path>`. Returns `""` on error or sandbox violation. |
| `(system-net-send! cid msg)` | `(net-send! cid msg)` | `(isize, String) -> bool` | Queues `NetEvent::Send { client_id: cid, message: msg }` to `ClientHub`. Returns `#true`. |
| `(system-net-disconnect! cid)` | `(net-disconnect! cid)` | `(isize) -> bool` | Queues `NetEvent::Disconnect { client_id: cid }` to `ClientHub`. Returns `#true`. |

---

## 4. Required Scheme Hooks & Execution Pipeline

A valid CardCraft Scheme script (`core.scm`) must define the following top-level functions:

### 4.1 Lifecycle & Rendering Hooks
1. **`(game-start!)`**
   - Called once immediately after `core.scm` is loaded.
2. **`(game-update! dt)`**
   - Called every 100ms (`dt = 0.1`) by the background ticker, or via `GameEngine::tick(dt)`.
   - Must advance timers and, if network clients need screen updates, push snapshots via `system-net-send!`.
3. **`(game-render)`**
   - Called by `GameEngine::get_snapshot()` (used by local TUI `cid = 0` and headless `VirtualScreen` tests).
   - Must return a valid JSON string matching `GameViewSnapshot`:
     ```json
     {
       "cursor": [11, 20],
       "grid": [
         "+-- Title ------------------+",
         "| \u001b[32m Colored Row \u001b[0m |"
       ]
     }
     ```
   - `grid` (or aliases `lines`, `screen`): `Vec<String>` of rows rendered top-to-bottom.
   - `cursor`: Optional `[x, y]` (`[u16; 2]`, 0-indexed column and row) for terminal cursor placement.
   - **ANSI Escape Sequences**: Standard VT100 sequences (`\x1b[0m`, `\x1b[1m`, `\x1b[2m`, `\x1b[30m`–`\x1b[37m`, `\x1b[90m`–`\x1b[97m`, `\x1b[38;5;Nm`, `\x1b[38;2;R;G;Bm`) are natively parsed by both `src/render.rs` (TUI) and `web/index.html` (Browser). Note: when embedding ESC (char 27) inside JSON strings in Scheme, escape it as `\u001b`.

### 4.2 Network & Unified Input Hooks
All client connections (`cid = 0` for local TUI, `cid >= 10` for TCP/WS/P2P clients) invoke:

1. **`(net-on-connect! cid)`**
   - Invoked when a client connects.
   - For network clients (`cid > 0`), Scheme should send the initial view and welcome message via `(system-net-send! cid ...)`.
2. **`(net-on-message! cid msg-str)`**
   - Invoked for all local TUI hardware events (`cid = 0`) and remote client text lines (`cid > 0`).
   - Exact hardware event formats generated by `src/main.rs`:
     - **Keyboard (`cid = 0`)**:
       - Printable chars: `"key:a"`, `"key:1"`, `"key:ctrl+x"`, `"key:space"`
       - Navigation/Control: `"key:enter"`, `"key:backspace"`, `"key:delete"`, `"key:esc"`, `"key:tab"`, `"key:up"`, `"key:down"`, `"key:left"`, `"key:right"`, `"key:home"`, `"key:end"`, `"key:pageup"`, `"key:pagedown"`, `"key:f1"`..`"key:f12"`
     - **Mouse (`cid = 0`)**: `"mouse:<kind>:<column>:<row>"` where `<kind>` is `"down:left"`, `"up:left"`, `"drag:left"`, `"move"`, `"scrolldown"`, `"scrollup"`.
     - **Resize (`cid = 0`)**: `"resize:<cols>:<rows>"` (e.g. `"resize:100:30"`).
     - **Lisp God Mode (`cid = 0`)**: If `client_id == 0` and `text.trim().starts_with('(')`, `EngineCommand::Message` evaluates `text` directly via `engine.eval_raw(text)` instead of calling `net-on-message!`.
3. **`(net-on-disconnect! cid)`**
   - Invoked when a client disconnects.
4. **`(game-input . rest)`** *(Optional fallback)*
   - `GameEngine::execute_command(input)` calls `net_message(0, input)` first, and only falls back to `(game-input '("tok1" "tok2"))` if `net-on-message!` is not defined.

---

## 5. Multi-Client / Multi-Map RPG Pattern in Scheme

Because Rust's network layer only forwards `(system-net-send! cid msg)`, per-player views (e.g., an RPG where players are on different maps) are handled entirely in Scheme:

```scheme
(define ESC (string (integer->char 27)))
(define *players* '()) ;; ((cid map-id x y) ...)

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

;; Render view specific to a given client-id
(define (render-for-client cid)
  (let* ([p (assoc cid *players*)]
         [map-name (if p (cadr p) "Town")]
         [px (if p (caddr p) 1)]
         [py (if p (cadddr p) 1)]
         [row0 (string-append "=== Map: " map-name " (Player #" (number->string cid) ") ===")]
         [row1 (string-append "Pos: (" (number->string px) ", " (number->string py) ")")])
    (string-append
      "{\"cursor\":[" (number->string px) "," (number->string py) "],"
      "\"grid\":[\"" (escape-json-str row0) "\",\"" (escape-json-str row1) "\"]}")))

;; Default (game-render) for local TUI (cid = 0)
(define (game-render)
  (render-for-client 0))

;; Push view to a specific network client
(define (send-view! cid)
  (when (> cid 0)
    (system-net-send! cid (render-for-client cid))))

(define (game-start!)
  (set! *players* (list (list 0 "Town" 1 1)))
  #t)

(define (game-update! dt) #t)

(define (net-on-connect! cid)
  (set! *players* (cons (list cid "Forest" 2 2) *players*))
  (send-view! cid)
  "OK")

(define (net-on-message! cid msg)
  ;; Update only `cid`'s state and push `cid`'s map view
  (send-view! cid)
  "OK")

(define (net-on-disconnect! cid) "OK")
```

---

## 6. Verification & Automated Testing Commands

After modifying Rust or Scheme code, always run:

```bash
# 1. Run headless self-test (prints rendered 90x24 VirtualScreen frame)
cargo run -- --test

# 2. Run all 24 unit, rendering, TCP, WebSocket, and P2P integration tests
cargo test
```
