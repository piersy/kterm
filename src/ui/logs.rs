use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let follow_indicator = if app.log_follow { " [FOLLOW] " } else { "" };
    let title = format!(
        " Logs{} ({} lines) ",
        follow_indicator,
        app.log_lines.len()
    );

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    if app.log_lines.is_empty() {
        let text = if app.loading {
            "Waiting for logs..."
        } else {
            "No log output"
        };
        let paragraph = Paragraph::new(text).block(block);
        frame.render_widget(paragraph, area);
        return;
    }

    let lines: Vec<Line> = app
        .log_lines
        .iter()
        .map(|line| {
            let style = if line.contains("ERROR") || line.contains("error") {
                Style::default().fg(Color::Red)
            } else if line.contains("WARN") || line.contains("warn") {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            Line::from(Span::styled(line.as_str(), style))
        })
        .collect();

    let scroll = if app.log_follow {
        let total = lines.len() as u16;
        let visible = area.height.saturating_sub(2); // account for border
        total.saturating_sub(visible)
    } else {
        app.log_scroll
    };

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}
