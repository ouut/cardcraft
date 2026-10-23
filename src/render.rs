use ratatui::{
    layout::Position,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::view::GameViewSnapshot;

/// 100% Scheme-driven full-screen canvas rendering
pub fn render_ui(frame: &mut Frame, snapshot: &GameViewSnapshot) {
    let area = frame.area();
    let rendered_lines: Vec<Line> = snapshot.grid.iter().map(|l| parse_grid_line(l)).collect();
    let paragraph = Paragraph::new(rendered_lines);
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

/// Parses bracketed color tags such as `[red]@`, `[yellow]$`, `[cyan]...[reset]`
pub fn parse_grid_line(line: &str) -> Line<'static> {
    if !line.contains('[') {
        return Line::from(line.to_string());
    }

    let mut spans = Vec::new();
    let mut current_style = Style::default();
    let mut rest = line;

    while let Some(start_bracket) = rest.find('[') {
        if start_bracket > 0 {
            spans.push(Span::styled(rest[..start_bracket].to_string(), current_style));
        }
        let after_bracket = &rest[start_bracket + 1..];
        if let Some(end_bracket) = after_bracket.find(']') {
            let tag = &after_bracket[..end_bracket];
            match tag.to_lowercase().as_str() {
                "red" => current_style = Style::default().fg(Color::Red),
                "green" => current_style = Style::default().fg(Color::Green),
                "yellow" => current_style = Style::default().fg(Color::Yellow),
                "blue" => current_style = Style::default().fg(Color::Blue),
                "magenta" => current_style = Style::default().fg(Color::Magenta),
                "cyan" => current_style = Style::default().fg(Color::Cyan),
                "white" => current_style = Style::default().fg(Color::White),
                "gray" | "darkgray" => current_style = Style::default().fg(Color::DarkGray),
                "bold" => current_style = current_style.add_modifier(Modifier::BOLD),
                "reset" => current_style = Style::default(),
                _ => {
                    spans.push(Span::styled(format!("[{}]", tag), current_style));
                }
            }
            rest = &after_bracket[end_bracket + 1..];
        } else {
            spans.push(Span::styled(rest[start_bracket..].to_string(), current_style));
            rest = "";
            break;
        }
    }

    if !rest.is_empty() {
        spans.push(Span::styled(rest.to_string(), current_style));
    }

    Line::from(spans)
}
