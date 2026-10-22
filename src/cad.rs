#[derive(Debug, Clone)]
pub struct CommandState {
    pub input: String,
    pub cursor_position: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub last_feedback: Option<String>,
}

impl Default for CommandState {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_position: 0,
            history: Vec::new(),
            history_index: None,
            last_feedback: None,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    pub fn delete_backspace(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
            self.input.remove(self.cursor_position);
        }
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position < self.input.len() {
            self.input.remove(self.cursor_position);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input.len() {
            self.cursor_position += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor_position = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_position = self.input.len();
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let new_idx = match self.history_index {
            None => self.history.len().saturating_sub(1),
            Some(i) => i.saturating_sub(1),
        };
        self.history_index = Some(new_idx);
        self.input = self.history[new_idx].clone();
        self.cursor_position = self.input.len();
    }

    pub fn history_down(&mut self) {
        if let Some(i) = self.history_index {
            if i + 1 < self.history.len() {
                let new_idx = i + 1;
                self.history_index = Some(new_idx);
                self.input = self.history[new_idx].clone();
            } else {
                self.history_index = None;
                self.input.clear();
            }
            self.cursor_position = self.input.len();
        }
    }

    pub fn submit(&mut self) -> String {
        let cmd = self.input.trim().to_string();
        if !cmd.is_empty() {
            self.history.push(cmd.clone());
        }
        self.input.clear();
        self.cursor_position = 0;
        self.history_index = None;
        cmd
    }

    pub fn set_feedback(&mut self, msg: String) {
        self.last_feedback = Some(msg);
    }
}
