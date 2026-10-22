use ratatui::{
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::cad::CommandState;
use crate::view::GameViewSnapshot;

pub fn render_ui(frame: &mut Frame, snapshot: &GameViewSnapshot, cmd: &CommandState) {
    let size = frame.area();
    let show_panels = !snapshot.panels.is_empty();

    let mut constraints = vec![
        Constraint::Length(3), // Header
        Constraint::Min(6),    // Board
    ];
    if show_panels {
        constraints.push(Constraint::Length(3)); // Actions / Panels
    }
    constraints.push(Constraint::Length(5)); // Logs
    constraints.push(Constraint::Length(3)); // Command Bar

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    render_header(frame, chunks[0], snapshot);
    render_board(frame, chunks[1], snapshot);

    if show_panels {
        render_panels(frame, chunks[2], snapshot);
        render_logs(frame, chunks[3], snapshot);
        render_command_bar(frame, chunks[4], cmd);
    } else {
        render_logs(frame, chunks[2], snapshot);
        render_command_bar(frame, chunks[3], cmd);
    }
}

fn render_header(frame: &mut Frame, area: Rect, snapshot: &GameViewSnapshot) {
    let title = snapshot.title.as_deref().unwrap_or("CardCraft Console");

    let mut spans = Vec::new();

    if !snapshot.metrics.is_empty() {
        // 1. Dynamic metric badges defined by Scheme
        for (i, m) in snapshot.metrics.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" | "));
            }
            let color = match m.color.as_deref() {
                Some("red") => Color::Red,
                Some("green") => Color::Green,
                Some("yellow") => Color::Yellow,
                Some("cyan") => Color::Cyan,
                Some("magenta") => Color::Magenta,
                Some("blue") => Color::Blue,
                Some("white") => Color::White,
                _ => Color::LightCyan,
            };
            spans.push(Span::styled(
                format!(" {}: {} ", m.label, m.value),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
    } else if snapshot.meta.moon > 0 || snapshot.meta.gold > 0 || snapshot.meta.card_count > 0 {
        // 2. Legacy Stacklands header fallback
        let meta = &snapshot.meta;
        let food_style = if meta.food_current < meta.food_required {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };

        let card_style = if meta.card_count > meta.card_limit {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let status_style = if meta.status == "RUNNING" {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        };

        let total_bar_chars = 10;
        let filled = ((meta.day_progress * total_bar_chars as f64).round() as usize).min(total_bar_chars);
        let empty = total_bar_chars.saturating_sub(filled);
        let time_bar = format!("[{}{}]", "=".repeat(filled), ".".repeat(empty));

        spans.push(Span::styled(format!(" MOON {} ", meta.moon), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(format!("Day: {} {:.0}s", time_bar, meta.day_seconds_left), Style::default().fg(Color::Yellow)));
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(format!("Food: {}/{}", meta.food_current, meta.food_required), food_style));
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(format!("Gold: {}G", meta.gold), Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD)));
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(format!("Cards: {}/{}", meta.card_count, meta.card_limit), card_style));
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(format!("[{}]", meta.status), status_style));
    } else {
        // 3. Minimal default header
        spans.push(Span::styled(
            format!(" {} ", title),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            format!("[{}]", snapshot.meta.status),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", title));
    let paragraph = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(paragraph, area);
}

/// Generic, dynamic card/item color styling
/// Never hardcodes specific card names. Uses explicit color tags or deterministic hashing.
fn card_style(card: &str) -> Style {
    // 1. Explicit color tags: e.g. "[red]Dragon" or "[green]Berry"
    if let Some(rest) = card.strip_prefix('[') {
        if let Some((color_name, _)) = rest.split_once(']') {
            let color = match color_name.to_lowercase().as_str() {
                "red" => Color::Red,
                "green" => Color::Green,
                "yellow" => Color::Yellow,
                "blue" => Color::Blue,
                "magenta" => Color::Magenta,
                "cyan" => Color::Cyan,
                "gray" | "darkgray" => Color::DarkGray,
                _ => Color::White,
            };
            return Style::default().fg(color).add_modifier(Modifier::BOLD);
        }
    }

    // 2. Special indicators
    if card.starts_with("Pack:") {
        return Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD);
    }
    if card == "Coin" || card.ends_with('G') {
        return Style::default().fg(Color::LightYellow);
    }

    // 3. Deterministic hash-based palette mapping for ANY custom card name
    let mut hash: u32 = 5381;
    for b in card.bytes() {
        hash = ((hash << 5).wrapping_add(hash)).wrapping_add(b as u32);
    }
    const PALETTE: [Color; 7] = [
        Color::Cyan,
        Color::Green,
        Color::Yellow,
        Color::Magenta,
        Color::LightCyan,
        Color::LightGreen,
        Color::LightYellow,
    ];
    let color = PALETTE[(hash as usize) % PALETTE.len()];
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn render_board(frame: &mut Frame, area: Rect, snapshot: &GameViewSnapshot) {
    let mut items = Vec::new();

    for s in &snapshot.stacks {
        let mut spans = Vec::new();

        // Stack ID: e.g. [ 1]
        spans.push(Span::styled(
            format!("[{:2}] ", s.id),
            Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD),
        ));

        // Items/Cards list
        for (i, card) in s.cards.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(card.clone(), card_style(card)));
        }

        // Progress bar if active
        if let Some(prog) = s.progress {
            let total = s.total_time.unwrap_or(1.0);
            let bar_len = 8;
            let filled = ((prog * bar_len as f64).round() as usize).min(bar_len);
            let empty = bar_len.saturating_sub(filled);
            let bar_str = format!("  [{}{}]", "=".repeat(filled), ".".repeat(empty));

            spans.push(Span::styled(bar_str, Style::default().fg(Color::Yellow)));

            if let Some(ref rname) = s.recipe_name {
                spans.push(Span::styled(
                    format!(" ({:.1}s / {})", (1.0 - prog).max(0.0) * total, rname),
                    Style::default().fg(Color::LightYellow),
                ));
            }
        } else if s.status != "Idle" {
            spans.push(Span::styled(
                format!(" [{}]", s.status),
                Style::default().fg(Color::DarkGray),
            ));
        }

        items.push(ListItem::new(Line::from(spans)));
    }

    if items.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            " (Empty board) ",
            Style::default().fg(Color::DarkGray),
        ))));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Board & Slots ");
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_panels(frame: &mut Frame, area: Rect, snapshot: &GameViewSnapshot) {
    let mut spans = vec![
        Span::styled("ACTIONS: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ];

    for (i, item) in snapshot.panels.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  |  "));
        }
        let text = if let Some(cost) = item.cost {
            format!("[b {}] {} ({}G)", item.id, item.name, cost)
        } else if let Some(ref desc) = item.desc {
            format!("[{}] {} ({})", item.id, item.name, desc)
        } else {
            format!("[{}] {}", item.id, item.name)
        };
        spans.push(Span::styled(text, Style::default().fg(Color::White)));
    }

    let title = snapshot.panel_title.as_deref().unwrap_or(" Quick Actions & Store ");
    let block = Block::default().borders(Borders::ALL).title(format!(" {} ", title));
    let paragraph = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(paragraph, area);
}

fn render_logs(frame: &mut Frame, area: Rect, snapshot: &GameViewSnapshot) {
    let mut items = Vec::new();
    let max_lines = area.height.saturating_sub(2) as usize;

    let skip_count = snapshot.logs.len().saturating_sub(max_lines);
    for log in snapshot.logs.iter().skip(skip_count) {
        let style = if log.contains("Starved") || log.contains("Error") {
            Style::default().fg(Color::Red)
        } else if log.contains("Produced") || log.contains("Born") || log.contains("Harvest") {
            Style::default().fg(Color::Green)
        } else if log.contains("Stacked") || log.contains("Bought") {
            Style::default().fg(Color::LightYellow)
        } else {
            Style::default().fg(Color::Gray)
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled("• ", Style::default().fg(Color::DarkGray)),
            Span::styled(log.clone(), style),
        ])));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" System Event Log & Chat ");
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_command_bar(frame: &mut Frame, area: Rect, cmd: &CommandState) {
    let feedback = cmd.last_feedback.as_deref().unwrap_or("");
    let title = if feedback.is_empty() {
        " Command Prompt (CAD style / '(' for Lisp REPL) ".to_string()
    } else {
        format!(" Command Prompt [{}] ", feedback)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title);

    let prompt_line = Line::from(vec![
        Span::styled("Command: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(&cmd.input),
    ]);

    let paragraph = Paragraph::new(prompt_line).block(block);
    frame.render_widget(paragraph, area);

    // Position hardware cursor inside the command bar
    let cursor_x = area.x + 10 + cmd.cursor_position as u16;
    let cursor_y = area.y + 1;
    if cursor_x < area.x + area.width - 1 {
        frame.set_cursor_position(Position { x: cursor_x, y: cursor_y });
    }
}
