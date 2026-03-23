use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::types::{ConfirmAction, Focus, ViewMode};

fn resource_list_bindings(app: &App) -> String {
    let mut parts = vec!["q:Quit", "C:Cluster", "N:Namespace", "T:Type", "j/k:Nav", "Enter:Detail"];
    let rt = app.selected_row_resource_type();
    if rt.map(|t| t.supports_logs()).unwrap_or(app.primary_resource_type().supports_logs()) {
        parts.push("l:Logs");
    }
    parts.push("d:Delete");
    if rt.map(|t| t.supports_restart()).unwrap_or(app.primary_resource_type().supports_restart()) {
        parts.push("r:Restart");
    }
    parts.push("e:Edit");
    parts.push("/:Filter");
    parts.push("Ctrl+F:Search");
    parts.join("  ")
}

fn detail_bindings(app: &App) -> String {
    let mut parts = vec!["Esc:Back", "j/k:Scroll", "e:Edit"];
    let rt = app.selected_row_resource_type();
    if rt.map(|t| t.supports_logs()).unwrap_or(false) {
        parts.push("l:Logs");
    }
    parts.push("d:Delete");
    if rt.map(|t| t.supports_restart()).unwrap_or(false) {
        parts.push("r:Restart");
    }
    parts.push("g/G:Top/Bottom");
    parts.join("  ")
}

fn search_detail_bindings(app: &App) -> String {
    let mut parts = vec!["Esc:Back to search", "j/k:Scroll"];
    if let Some(result) = app.selected_search_result() {
        if result.resource_type.supports_logs() {
            parts.push("l:Logs");
        }
    }
    parts.push("g/G:Top/Bottom");
    parts.join("  ")
}

pub fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let bindings_owned: String;
    let bindings: &str = match app.view_mode {
        ViewMode::List => {
            if app.filter_active {
                "Esc:Cancel  Enter:Apply  Type to filter..."
            } else if matches!(app.focus, Focus::Selector(_)) {
                "Esc:Close  Enter:Confirm  Space:Toggle  Up/Down:Nav  Type to filter..."
            } else {
                bindings_owned = resource_list_bindings(app);
                &bindings_owned
            }
        }
        ViewMode::Detail if app.entered_from_search => {
            bindings_owned = search_detail_bindings(app);
            &bindings_owned
        }
        ViewMode::Detail => {
            bindings_owned = detail_bindings(app);
            &bindings_owned
        }
        ViewMode::Logs if app.entered_from_search => {
            "Esc:Back to search  f:Follow  j/k:Scroll  g/G:Top/Bottom  o:Vim  O:Less"
        }
        ViewMode::Logs => "Esc:Back  f:Follow  j/k:Scroll  g/G:Top/Bottom  o:Vim  O:Less",
        ViewMode::Confirm(_) => "y:Confirm  Any other key:Cancel",
        ViewMode::Search => "Esc:Back  Down/Up:Nav  Enter:Detail  Type to search...",
    };

    let mut spans = vec![Span::styled(
        bindings.to_owned(),
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
