use ansi_to_tui::IntoText;
use ratatui::{layout::Position, text::Text, widgets::Paragraph, Frame};

use crate::view::GameViewSnapshot;

/// 100% Scheme-driven full-screen canvas rendering.
/// Renders standard ANSI escape strings directly, maintaining complete styling neutrality.
pub fn render_ui(frame: &mut Frame, snapshot: &GameViewSnapshot) {
    let area = frame.area();
    let raw_text = snapshot.grid.join("\n");
    let text = raw_text
        .as_bytes()
        .into_text()
        .unwrap_or_else(|_| Text::from(raw_text));

    let paragraph = Paragraph::new(text);
    frame.render_widget(paragraph, area);

    if let Some([cx, cy]) = snapshot.cursor {
        if cx < area.width && cy < area.height {
            frame.set_cursor_position(Position {
                x: area.x + cx,
                y: area.y + cy,
            });
        }
    }
}
