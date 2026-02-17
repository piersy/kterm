use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::types::Focus;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::horizontal([
        Constraint::Percentage(33),
        Constraint::Percentage(34),
        Constraint::Percentage(33),
    ])
    .split(area);

    render_selector(
        frame,
        "Context",
        &app.contexts,
        app.selected_context,
        app.focus == Focus::ContextSelector,
        if app.focus == Focus::ContextSelector {
            Some(&app.dropdown_query)
        } else {
            None
        },
        chunks[0],
    );

    render_selector(
        frame,
        "Namespace",
        &app.namespaces,
        app.selected_namespace,
        app.focus == Focus::NamespaceSelector,
        if app.focus == Focus::NamespaceSelector {
            Some(&app.dropdown_query)
        } else {
            None
        },
        chunks[1],
    );

    let type_names: Vec<String> = crate::types::ResourceType::ALL
        .iter()
        .map(|t| t.to_string())
        .collect();
    let type_idx = crate::types::ResourceType::ALL
        .iter()
        .position(|t| *t == app.resource_type)
        .unwrap_or(0);

    render_selector(
        frame,
        "Type",
        &type_names,
        type_idx,
        app.focus == Focus::ResourceTypeSelector,
        if app.focus == Focus::ResourceTypeSelector {
            Some(&app.dropdown_query)
        } else {
            None
        },
        chunks[2],
    );
}

fn render_selector(
    frame: &mut Frame,
    title: &str,
    items: &[String],
    selected: usize,
    focused: bool,
    query: Option<&str>,
    area: Rect,
) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(border_style);

    if let Some(q) = query {
        // Focused: show search input with cursor
        let display = format!("{}\u{2588}", q);
        let paragraph = Paragraph::new(display)
            .block(block)
            .style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, area);
    } else {
        // Unfocused: show current value
        let value = items.get(selected).map(|s| s.as_str()).unwrap_or("—");

        let line = Line::from(vec![Span::styled(
            value,
            Style::default().fg(Color::DarkGray),
        )]);

        let paragraph = Paragraph::new(line).block(block).centered();
        frame.render_widget(paragraph, area);
    }
}

pub fn render_dropdown(frame: &mut Frame, app: &App, area: Rect) {
    let items = app.dropdown_items();

    // Build the list items from the filtered indices
    let list_items: Vec<ListItem> = app
        .dropdown_filtered
        .iter()
        .map(|&idx| {
            let name = items.get(idx).map(|s| s.as_str()).unwrap_or("?");
            ListItem::new(name.to_string())
        })
        .collect();

    let title = if app.dropdown_query.is_empty() {
        format!(" {} items ", app.dropdown_filtered.len())
    } else {
        format!(
            " {} matching ",
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
        .highlight_symbol("▶ ");

    let mut state = ListState::default();
    if !app.dropdown_filtered.is_empty() {
        state.select(Some(app.dropdown_selected));
    }

    frame.render_stateful_widget(list, area, &mut state);
}
