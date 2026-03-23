use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::types::{Focus, SelectorTarget};

/// Render the three stacked selector rows (Cluster, Namespace, Type).
pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let is_active = |target: SelectorTarget| matches!(app.focus, Focus::Selector(t) if t == target);

    // Cluster row
    render_selector_row(
        frame,
        "Cluster",
        &selected_context_names(app),
        is_active(SelectorTarget::Context),
        chunks[0],
    );

    // Namespace row
    render_selector_row(
        frame,
        "Namespace",
        &selected_namespace_names(app),
        is_active(SelectorTarget::Namespace),
        chunks[1],
    );

    // Type row
    render_selector_row(
        frame,
        "Type",
        &selected_type_names(app),
        is_active(SelectorTarget::ResourceType),
        chunks[2],
    );
}

fn selected_context_names(app: &App) -> String {
    let mut names: Vec<&str> = app
        .selected_contexts
        .iter()
        .filter_map(|&idx| app.contexts.get(idx).map(|s| s.as_str()))
        .collect();
    names.sort();
    if names.is_empty() {
        "\u{2014}".to_string()
    } else {
        names.join(", ")
    }
}

fn selected_namespace_names(app: &App) -> String {
    let mut names: Vec<&str> = app
        .selected_namespaces
        .iter()
        .filter_map(|&idx| app.namespaces.get(idx).map(|s| s.as_str()))
        .collect();
    names.sort();
    if names.is_empty() {
        "\u{2014}".to_string()
    } else {
        names.join(", ")
    }
}

fn selected_type_names(app: &App) -> String {
    if app.selected_resource_types.is_empty() {
        "\u{2014}".to_string()
    } else {
        app.selected_resource_types
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn render_selector_row(
    frame: &mut Frame,
    label: &str,
    value: &str,
    active: bool,
    area: Rect,
) {
    let label_style = if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let value_style = if active {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let hotkey = match label {
        "Cluster" => "C",
        "Namespace" => "N",
        "Type" => "T",
        _ => "",
    };

    let line = Line::from(vec![
        Span::styled(format!(" [{}] ", hotkey), Style::default().fg(Color::Yellow)),
        Span::styled(format!("{}: ", label), label_style),
        Span::styled(value, value_style),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

/// Render the dropdown overlay for the active selector.
/// This is rendered as a floating overlay on top of whatever is below.
pub fn render_dropdown(frame: &mut Frame, app: &App, area: Rect) {
    let items = app.dropdown_items();

    let list_items: Vec<ListItem> = app
        .dropdown_filtered
        .iter()
        .map(|&idx| {
            let name = items.get(idx).map(|s| s.as_str()).unwrap_or("?");
            let is_toggled = app.dropdown_toggled.contains(&idx);
            let prefix = if is_toggled { "[x] " } else { "[ ] " };
            let style = if is_toggled {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(name.to_string(), Style::default()),
            ]))
        })
        .collect();

    let selector_name = match app.focus {
        Focus::Selector(SelectorTarget::Context) => "Cluster",
        Focus::Selector(SelectorTarget::Namespace) => "Namespace",
        Focus::Selector(SelectorTarget::ResourceType) => "Type",
        _ => "",
    };

    let title = if app.dropdown_query.is_empty() {
        format!(" {} \u{2500} {} items ", selector_name, app.dropdown_filtered.len())
    } else {
        format!(
            " {} \u{2500} filter: {}\u{2588} \u{2500} {} matching ",
            selector_name,
            app.dropdown_query,
            app.dropdown_filtered.len()
        )
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let highlight_style = Style::default()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    let list = List::new(list_items)
        .block(block)
        .highlight_style(highlight_style)
        .highlight_symbol("\u{25b6} ");

    let mut state = ListState::default();
    if !app.dropdown_filtered.is_empty() {
        state.select(Some(app.dropdown_selected));
    }

    frame.render_stateful_widget(list, area, &mut state);
}
