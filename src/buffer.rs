use anyhow::Result;
use ratatui::{backend::TestBackend, Terminal};

use crate::render::render_ui;
use crate::view::GameViewSnapshot;

/// Headless Virtual Buffer layer for decoupled rendering.
/// Can be used for automated testing, CI/CD, network streaming, or Web/WASM canvas rendering.
pub struct VirtualScreen {
    terminal: Terminal<TestBackend>,
}

impl VirtualScreen {
    pub fn new(width: u16, height: u16) -> Result<Self> {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        let backend = TestBackend::new(width, height);
        let term = Terminal::new(backend).expect("Failed to resize virtual buffer");
        self.terminal = term;
    }

    pub fn render(&mut self, snapshot: &GameViewSnapshot) -> Result<()> {
        self.terminal.draw(|frame| {
            render_ui(frame, snapshot);
        })?;
        Ok(())
    }

    /// Extract rendered ASCII frame row-by-row into a formatted string
    pub fn to_string(&self) -> String {
        let buffer = self.terminal.backend().buffer();
        let mut output = String::new();
        for y in 0..buffer.area.height {
            let mut row = String::new();
            for x in 0..buffer.area.width {
                row.push_str(buffer[(x, y)].symbol());
            }
            output.push_str(row.trim_end());
            output.push('\n');
        }
        output
    }

    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }

    pub fn get_row(&self, y: u16) -> Option<String> {
        let buffer = self.terminal.backend().buffer();
        if y >= buffer.area.height {
            return None;
        }
        let mut row = String::new();
        for x in 0..buffer.area.width {
            row.push_str(buffer[(x, y)].symbol());
        }
        Some(row.trim_end().to_string())
    }

    pub fn raw_buffer(&self) -> &ratatui::buffer::Buffer {
        self.terminal.backend().buffer()
    }
}
