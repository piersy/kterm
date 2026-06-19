use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
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
        parts.push("R:Restart");
    }
    if rt.map(|t| t.supports_scale()).unwrap_or(app.primary_resource_type().supports_scale()) {
        parts.push("s:Scale");
    }
    parts.push("r:Related");
    parts.push("e:Edit");
    if rt.map(|t| t.supports_exec()).unwrap_or(app.primary_resource_type().supports_exec()) {
        parts.push("x:Exec");
    }
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
        parts.push("R:Restart");
    }
    if rt.map(|t| t.supports_exec()).unwrap_or(false) {
        parts.push("x:Exec");
    }
    parts.push("g/G:Top/Bottom");
    parts.join("  ")
}

fn related_bindings(app: &App) -> String {
    // The related list is fully interactive — same per-resource actions as the
    // normal list, gated on the highlighted row's type, minus `r` (we are
    // already viewing related components).
    let mut parts = vec!["Esc:Back", "j/k:Nav", "Enter:Detail"];
    let rt = app.selected_row_resource_type();
    if rt.map(|t| t.supports_logs()).unwrap_or(false) {
        parts.push("l:Logs");
    }
    parts.push("d:Delete");
    if rt.map(|t| t.supports_restart()).unwrap_or(false) {
        parts.push("R:Restart");
    }
    if rt.map(|t| t.supports_scale()).unwrap_or(false) {
        parts.push("s:Scale");
    }
    parts.push("e:Edit");
    if rt.map(|t| t.supports_exec()).unwrap_or(false) {
        parts.push("x:Exec");
    }
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
        ViewMode::Related => {
            bindings_owned = related_bindings(app);
            &bindings_owned
        }
        ViewMode::Scale => "Type a number  Enter:Apply  Backspace:Edit  Esc:Cancel",
        ViewMode::Search => "Esc:Back  Down/Up:Nav  Enter:Detail  Type to search...",
    };

    let line = Line::from(Span::styled(
        bindings.to_owned(),
        Style::default().fg(Color::DarkGray),
    ));
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

pub fn render_scale_dialog(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let popup_area = centered_rect(50, 8, area);

    frame.render_widget(Clear, popup_area);

    // Borrow the name (no clone) and use the replica count captured when the
    // popup opened, so this render path does no per-frame YAML parsing.
    let name = app
        .selected_resource()
        .map(|(res, _)| res.name.as_str())
        .unwrap_or("");
    let current_str = app
        .scale_current
        .map(|r| r.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let text = format!(
        "Scale '{}'\n\nCurrent replicas: {}\nNew replicas: {}\n\nPress Enter to apply, Esc to cancel.",
        name, current_str, app.scale_input
    );

    let block = Block::default()
        .title(" Scale ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(text)
        .block(block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, popup_area);
}

pub fn render_error_popup(frame: &mut Frame, app: &App) {
    if !app.error_popup {
        return;
    }
    if let Some(ref msg) = app.error_message {
        let area = frame.area();
        // Compute height: 2 lines border + message lines (wrap at ~60% width)
        let popup_width = (area.width as f32 * 0.6).max(30.0).min(area.width as f32) as u16;
        let inner_width = popup_width.saturating_sub(4) as usize;
        let wrapped_lines = msg.len().checked_div(inner_width).map(|d| d + 1).unwrap_or(1);
        let popup_height = (wrapped_lines as u16 + 4).min(area.height);

        let popup_area = centered_rect_fixed(popup_width, popup_height, area);
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .title(" Error ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));

        let text = format!("{}\n\nPress any key to dismiss", msg);
        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Color::White))
            .wrap(ratatui::widgets::Wrap { trim: false });

        frame.render_widget(paragraph, popup_area);
    }
}

fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
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
