use crate::app::{App, Role};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(f.area());

    draw_chat(f, app, chunks[0]);
    draw_input(f, app, chunks[1]);
}

fn draw_chat(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let inner_width = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        match msg.role {
            Role::User => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Kamu: ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(msg.content.clone()),
                ]));
                lines.push(Line::raw(""));
            }
            Role::Ai => {
                lines.push(Line::from(Span::styled(
                    "AI:",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )));
                for line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(Color::White),
                    )));
                }
                lines.push(Line::raw(""));
            }
            Role::System => {
                lines.push(Line::from(Span::styled(
                    msg.content.clone(),
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::raw(""));
            }
        }
    }

    if app.is_loading {
        lines.push(Line::from(Span::styled(
            format!("{} Sedang berpikir...", app.spinner()),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
    }

    let viewport_height = area.height.saturating_sub(2) as usize;
    let total_lines = estimate_line_count(&lines, inner_width);
    let scroll = total_lines.saturating_sub(viewport_height) as u16;

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" AI Personal Assistant "),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    f.render_widget(paragraph, area);
}

fn draw_input(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let (text, style) = if app.input.is_empty() && !app.is_loading {
        (
            "ketik pesan di sini...".to_string(),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        (app.input.clone(), Style::default().fg(Color::White))
    };

    let paragraph = Paragraph::new(text)
        .style(style)
        .block(Block::default().borders(Borders::ALL).title(" Input "));

    f.render_widget(paragraph, area);

    if !app.is_loading {
        let inner = area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        });
        let cursor_x = inner.x + (app.input.len() as u16).min(inner.width.saturating_sub(1));
        f.set_cursor_position((cursor_x, inner.y));
    }
}

fn estimate_line_count(lines: &[Line], width: usize) -> usize {
    if width == 0 {
        return lines.len();
    }
    lines
        .iter()
        .map(|line| {
            let len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
            if len == 0 { 1 } else { len.div_ceil(width) }
        })
        .sum()
}
