use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
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
        chunks[0],
    );

    render_selector(
        frame,
        "Namespace",
        &app.namespaces,
        app.selected_namespace,
        app.focus == Focus::NamespaceSelector,
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
        chunks[2],
    );
}

fn render_selector(
    frame: &mut Frame,
    title: &str,
    items: &[String],
    selected: usize,
    focused: bool,
    area: Rect,
) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let value = items.get(selected).map(|s| s.as_str()).unwrap_or("—");

    let arrow_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let line = Line::from(vec![
        Span::styled("◀ ", arrow_style),
        Span::styled(
            value,
            Style::default().add_modifier(if focused {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        ),
        Span::styled(" ▶", arrow_style),
    ]);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(border_style);

    let paragraph = Paragraph::new(line).block(block).centered();

    frame.render_widget(paragraph, area);
}
