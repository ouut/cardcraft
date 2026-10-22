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
}

impl GameEngine {
    pub fn new() -> Result<Self> {
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

        vm.register_fn("system-file-write", |path: String, content: String| -> bool {
            std::fs::write(path, content).is_ok()
        });

        vm.register_fn("system-file-read", |path: String| -> String {
            std::fs::read_to_string(path).unwrap_or_default()
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

        vm.run(CORE_SCRIPT)
            .map_err(|e| anyhow!("Failed to compile core.scm: {:?}", e))?;
        
        vm.run("(game-start!)")
            .map_err(|e| anyhow!("Failed to initialize game engine: {:?}", e))?;

        Ok(Self { vm, outbox })
    }

    pub fn drain_net_events(&self) -> Vec<NetEvent> {
        let mut box_lock = self.outbox.lock().unwrap();
        std::mem::take(&mut *box_lock)
    }

    pub fn net_connect(&mut self, client_id: isize) -> Result<String> {
        let code = format!("(on-net-connect! {})", client_id);
        self.eval_raw(&code)
    }

    pub fn net_message(&mut self, client_id: isize, msg: &str) -> Result<String> {
        let escaped = msg.replace('\\', "\\\\").replace('\"', "\\\"");
        let code = format!("(on-net-message! {} \"{}\")", client_id, escaped);
        self.eval_raw(&code)
    }

    pub fn net_disconnect(&mut self, client_id: isize) -> Result<String> {
        let code = format!("(on-net-disconnect! {})", client_id);
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
