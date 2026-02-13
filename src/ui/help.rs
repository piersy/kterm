use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::types::{ConfirmAction, ViewMode};

pub fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let bindings = match app.view_mode {
        ViewMode::List => {
            if app.filter_active {
                "Esc:Cancel  Enter:Apply  Type to filter..."
            } else {
                "q:Quit  Tab:Selector  j/k:Nav  Enter:Detail  l:Logs  d:Delete  r:Restart  e:Edit  /:Filter"
            }
        }
        ViewMode::Detail => "Esc:Back  j/k:Scroll  e:Edit  l:Logs  d:Delete  r:Restart  g/G:Top/Bottom",
        ViewMode::Logs => "Esc:Back  f:Follow  j/k:Scroll  g/G:Top/Bottom",
        ViewMode::Confirm(_) => "y:Confirm  Any other key:Cancel",
    };

    let mut spans = vec![Span::styled(
        bindings,
        Style::default().fg(Color::DarkGray),
    )];

    if let Some(ref err) = app.error_message {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            err.as_str(),
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);

    frame.render_widget(paragraph, area);
}

pub fn render_confirm_dialog(frame: &mut Frame, action: ConfirmAction) {
    let area = frame.area();
    let popup_area = centered_rect(50, 7, area);

    frame.render_widget(Clear, popup_area);

    let text = format!(
        "Are you sure you want to {} this resource?\n\nPress 'y' to confirm, any other key to cancel.",
        action.to_string().to_lowercase()
    );

    let block = Block::default()
        .title(format!(" Confirm {} ", action))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, popup_area);
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
