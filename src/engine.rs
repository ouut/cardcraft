use anyhow::{anyhow, Result};
use steel::steel_vm::engine::Engine;
use crate::view::GameViewSnapshot;

const CORE_SCRIPT: &str = include_str!("../scripts/core.scm");

pub struct GameEngine {
    vm: Engine,
}

impl GameEngine {
    pub fn new() -> Result<Self> {
        let mut vm = Engine::new();
        vm.run(CORE_SCRIPT)
            .map_err(|e| anyhow!("Failed to compile core.scm: {:?}", e))?;
        
        vm.run("(engine-init!)")
            .map_err(|e| anyhow!("Failed to initialize game engine: {:?}", e))?;

        Ok(Self { vm })
    }

    pub fn tick(&mut self, dt: f64) -> Result<()> {
        let code = format!("(engine-tick! {:.4})", dt);
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
            let res = self.vm.run(trimmed.to_string())
                .map_err(|e| anyhow!("Scheme eval error: {:?}", e))?;
            return Ok(format!("{:?}", res.last()));
        }

        // CAD command mode
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let tokens_str = tokens.iter()
            .map(|t| format!("\"{}\"", t.replace('\"', "\\\"")))
            .collect::<Vec<_>>()
            .join(" ");

        let code = format!("(handle-cmd '({}))", tokens_str);
        let res = self.vm.run(code)
            .map_err(|e| anyhow!("Command error: {:?}", e))?;

        if let Some(val) = res.last() {
            match val {
                steel::rvals::SteelVal::StringV(s) => Ok(s.to_string()),
                other => Ok(format!("{}", other)),
            }
        } else {
            Ok("OK".to_string())
        }
    }

    pub fn get_snapshot(&mut self) -> Result<GameViewSnapshot> {
        let res = self.vm.run("(engine-get-view-json)")
            .map_err(|e| anyhow!("Failed to get view json: {:?}", e))?;

        if let Some(steel::rvals::SteelVal::StringV(json_str)) = res.last() {
            let snapshot: GameViewSnapshot = serde_json::from_str(json_str)
                .map_err(|e| anyhow!("JSON deserialization failed: {:?}, raw JSON: {}", e, json_str))?;
            Ok(snapshot)
        } else {
            Err(anyhow!("engine-get-view-json did not return a string"))
        }
    }
}
