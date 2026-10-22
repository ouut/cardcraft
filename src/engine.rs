use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use anyhow::{anyhow, Result};
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;
use crate::view::GameViewSnapshot;

const CORE_SCRIPT: &str = include_str!("../scripts/core.scm");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEvent {
    Send { client_id: isize, message: String },
    Broadcast { message: String },
    Disconnect { client_id: isize },
}

pub struct GameEngine {
    vm: Engine,
    outbox: Arc<Mutex<Vec<NetEvent>>>,
    game_dir: PathBuf,
    sandbox_dir: PathBuf,
}

fn resolve_sandboxed_path(sandbox_dir: &Path, rel_path: &str) -> Option<PathBuf> {
    // Strip leading slashes to prevent root escaping
    let clean_rel = rel_path.trim_start_matches('/').trim_start_matches('\\');
    let candidate = sandbox_dir.join(clean_rel);

    let canonical_sandbox = sandbox_dir.canonicalize().ok()?;

    let parent = candidate.parent().unwrap_or(sandbox_dir);
    std::fs::create_dir_all(parent).ok();

    let canonical_target = if candidate.exists() {
        candidate.canonicalize().ok()?
    } else {
        let canonical_parent = parent.canonicalize().ok()?;
        canonical_parent.join(candidate.file_name()?)
    };

    if canonical_target.starts_with(&canonical_sandbox) {
        Some(canonical_target)
    } else {
        eprintln!("[SECURITY WARNING] Sandbox violation prevented: {:?}", rel_path);
        None
    }
}

impl GameEngine {
    pub fn new() -> Result<Self> {
        Self::with_game_dir(Path::new("."))
    }

    pub fn with_game_dir<P: AsRef<Path>>(game_dir_path: P) -> Result<Self> {
        let game_dir = game_dir_path.as_ref().to_path_buf();
        let sandbox_dir = game_dir.join("saves");
        std::fs::create_dir_all(&sandbox_dir).ok();

        let mut vm = Engine::new();
        let outbox = Arc::new(Mutex::new(Vec::new()));

        // Register Rust interfaces accessible to Scheme
        vm.register_fn("rust-log", |level: String, msg: String| {
            eprintln!("[{level}] {msg}");
        });

        vm.register_fn("rust-random-int", |min: i64, max: i64| -> i64 {
            if min >= max {
                return min;
            }
            rand::random_range(min..=max)
        });

        vm.register_fn("rust-millis", || -> i64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        });

        let sb_write = sandbox_dir.clone();
        vm.register_fn("system-file-write", move |path: String, content: String| -> bool {
            if let Some(target) = resolve_sandboxed_path(&sb_write, &path) {
                if let Some(p) = target.parent() {
                    std::fs::create_dir_all(p).ok();
                }
                std::fs::write(target, content).is_ok()
            } else {
                false
            }
        });

        let sb_read = sandbox_dir.clone();
        vm.register_fn("system-file-read", move |path: String| -> String {
            if let Some(target) = resolve_sandboxed_path(&sb_read, &path) {
                std::fs::read_to_string(target).unwrap_or_default()
            } else {
                String::new()
            }
        });

        // =========================================================================
        // The 4 Network & System Primitives registered for Scheme
        // =========================================================================
        let outbox_send = Arc::clone(&outbox);
        vm.register_fn("net-send!", move |client_id: isize, msg: String| -> bool {
            outbox_send.lock().unwrap().push(NetEvent::Send { client_id, message: msg });
            true
        });

        let outbox_bc = Arc::clone(&outbox);
        vm.register_fn("net-broadcast!", move |msg: String| -> bool {
            outbox_bc.lock().unwrap().push(NetEvent::Broadcast { message: msg });
            true
        });

        let outbox_dc = Arc::clone(&outbox);
        vm.register_fn("net-disconnect!", move |client_id: isize| -> bool {
            outbox_dc.lock().unwrap().push(NetEvent::Disconnect { client_id });
            true
        });

        vm.register_fn("system-time-now", || -> f64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0)
        });

        // Script loading with priority:
        // 1. <game_dir>/core.scm
        // 2. <game_dir>/main.scm
        // 3. Fallback to embedded CORE_SCRIPT
        let core_path = game_dir.join("core.scm");
        let main_path = game_dir.join("main.scm");
        let script_content = if core_path.is_file() {
            std::fs::read_to_string(&core_path)
                .map_err(|e| anyhow!("Failed to read {}: {:?}", core_path.display(), e))?
        } else if main_path.is_file() {
            std::fs::read_to_string(&main_path)
                .map_err(|e| anyhow!("Failed to read {}: {:?}", main_path.display(), e))?
        } else {
            CORE_SCRIPT.to_string()
        };

        vm.run(script_content)
            .map_err(|e| anyhow!("Failed to compile core.scm: {:?}", e))?;
        
        vm.run("(game-start!)")
            .map_err(|e| anyhow!("Failed to initialize game engine: {:?}", e))?;

        Ok(Self { vm, outbox, game_dir, sandbox_dir })
    }

    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    pub fn sandbox_dir(&self) -> &Path {
        &self.sandbox_dir
    }

    pub fn drain_net_events(&self) -> Vec<NetEvent> {
        let mut box_lock = self.outbox.lock().unwrap();
        std::mem::take(&mut *box_lock)
    }

    pub fn net_connect(&mut self, client_id: isize) -> Result<String> {
        let code = format!("(net-on-connect! {})", client_id);
        self.eval_raw(&code)
    }

    pub fn net_message(&mut self, client_id: isize, msg: &str) -> Result<String> {
        let escaped = msg.replace('\\', "\\\\").replace('\"', "\\\"");
        let code = format!("(net-on-message! {} \"{}\")", client_id, escaped);
        self.eval_raw(&code)
    }

    pub fn net_disconnect(&mut self, client_id: isize) -> Result<String> {
        let code = format!("(net-on-disconnect! {})", client_id);
        self.eval_raw(&code)
    }

    pub fn eval_raw(&mut self, code: &str) -> Result<String> {
        let res = self.vm.run(code.to_string())
            .map_err(|e| anyhow!("Scheme execution error: {:?}", e))?;
        if let Some(val) = res.last() {
            match val {
                steel::rvals::SteelVal::StringV(s) => Ok(s.to_string()),
                other => Ok(format!("{}", other)),
            }
        } else {
            Ok("()".to_string())
        }
    }

    pub fn tick(&mut self, dt: f64) -> Result<()> {
        let code = format!("(game-update! {:.4})", dt);
        self.vm.run(code)
            .map_err(|e| anyhow!("Tick error: {:?}", e))?;
        Ok(())
    }

    pub fn execute_command(&mut self, input: &str) -> Result<String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok("Empty command".to_string());
        }

        // Lisp/God mode if starts with '('
        if trimmed.starts_with('(') {
            return self.eval_raw(trimmed);
        }

        // CAD command mode
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let tokens_str = tokens.iter()
            .map(|t| format!("\"{}\"", t.replace('\"', "\\\"")))
            .collect::<Vec<_>>()
            .join(" ");

        let code = format!("(game-input '({}))", tokens_str);
        self.eval_raw(&code)
    }

    pub fn get_snapshot(&mut self) -> Result<GameViewSnapshot> {
        let res = self.vm.run("(game-render)")
            .map_err(|e| anyhow!("Failed to get view json: {:?}", e))?;

        if let Some(steel::rvals::SteelVal::StringV(json_str)) = res.last() {
            let snapshot: GameViewSnapshot = serde_json::from_str(json_str)
                .map_err(|e| anyhow!("JSON deserialization failed: {:?}, raw JSON: {}", e, json_str))?;
            Ok(snapshot)
        } else {
            Err(anyhow!("game-render did not return a string"))
        }
    }
}
