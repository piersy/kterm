use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let title = app
        .selected_resource()
        .map(|r| format!(" {} ", r.name))
        .unwrap_or_else(|| " Detail ".to_string());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let text = if app.detail_text.is_empty() {
        if app.loading {
            "Loading...".to_string()
        } else {
            "Press Enter on a resource to view details".to_string()
        }
    } else {
        app.detail_text.clone()
    };

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll, 0));

    frame.render_widget(paragraph, area);
}
