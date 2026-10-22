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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(8),    // Board (Stacks)
            Constraint::Length(3), // Shop & Hints
            Constraint::Length(5), // Event Logs
            Constraint::Length(3), // CAD Command Bar
        ])
        .split(size);

    render_header(frame, chunks[0], snapshot);
    render_board(frame, chunks[1], snapshot);
    render_shop(frame, chunks[2], snapshot);
    render_logs(frame, chunks[3], snapshot);
    render_command_bar(frame, chunks[4], cmd);
}

fn render_header(frame: &mut Frame, area: Rect, snapshot: &GameViewSnapshot) {
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

    // Day timer bar (e.g. 10 chars)
    let total_bar_chars = 10;
    let filled = ((meta.day_progress * total_bar_chars as f64).round() as usize).min(total_bar_chars);
    let empty = total_bar_chars.saturating_sub(filled);
    let time_bar = format!("[{}{}]", "=".repeat(filled), ".".repeat(empty));

    let header_line = Line::from(vec![
        Span::styled(format!(" MOON {} ", meta.moon), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(format!("Day: {} {:.0}s", time_bar, meta.day_seconds_left), Style::default().fg(Color::Yellow)),
        Span::raw(" | "),
        Span::styled(format!("Food: {}/{}", meta.food_current, meta.food_required), food_style),
        Span::raw(" | "),
        Span::styled(format!("Gold: {}G", meta.gold), Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(format!("Cards: {}/{}", meta.card_count, meta.card_limit), card_style),
        Span::raw(" | "),
        Span::styled(format!("[{}]", meta.status), status_style),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" CardCraft (Console Stacklands) ");
    let paragraph = Paragraph::new(header_line).block(block);
    frame.render_widget(paragraph, area);
}

fn card_style(card: &str) -> Style {
    match card {
        "Villager" => Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        "Baby" => Style::default().fg(Color::LightCyan),
        "BerryBush" | "Tree" | "Rock" => Style::default().fg(Color::Green),
        "Berry" | "Apple" | "CookedBerry" => Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        "Wood" | "Stone" => Style::default().fg(Color::DarkGray),
        "Campfire" | "House" | "Shed" => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        "Coin" => Style::default().fg(Color::LightYellow),
        c if c.starts_with("Pack:") => Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    }
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

        // Cards list
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
            " No cards on board. Type 'b basic' to buy a card pack.",
            Style::default().fg(Color::DarkGray),
        ))));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Active Stacks (Each Line = One Stack of Cards) ");
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_shop(frame: &mut Frame, area: Rect, snapshot: &GameViewSnapshot) {
    let mut spans = vec![
        Span::styled("SHOP: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ];

    for (i, item) in snapshot.shop.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  |  "));
        }
        spans.push(Span::styled(
            format!("[b {}] {} ({}G)", item.id, item.name, item.cost),
            Style::default().fg(Color::White),
        ));
    }

    spans.push(Span::raw("   |   "));
    spans.push(Span::styled(
        "CAD: s <from> <to> | u <id> | m <from> <to> | o <id> | x <id> | w [s] | p",
        Style::default().fg(Color::DarkGray),
    ));

    let block = Block::default().borders(Borders::ALL).title(" Quick Store & Shortcuts ");
    let paragraph = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(paragraph, area);
}

fn render_logs(frame: &mut Frame, area: Rect, snapshot: &GameViewSnapshot) {
    let mut log_lines = Vec::new();
    let display_logs = snapshot.logs.iter().take(3);

    for log in display_logs {
        let style = if log.contains("WARNING") || log.contains("starved") {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if log.contains("Produced:") || log.contains("Opened") {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Gray)
        };
        log_lines.push(Line::from(vec![
            Span::styled("• ", Style::default().fg(Color::Cyan)),
            Span::styled(log.clone(), style),
        ]));
    }

    let block = Block::default().borders(Borders::ALL).title(" Event Log ");
    let paragraph = Paragraph::new(log_lines).block(block);
    frame.render_widget(paragraph, area);
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
