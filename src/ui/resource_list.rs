use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let resource_type = app.resource_type;
    let headers = resource_type.column_headers();

    let header_cells: Vec<Cell> = headers
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
        .collect();
    let header_row = Row::new(header_cells).height(1);

    let filtered = app.filtered_resources();
    let rows: Vec<Row> = filtered
        .iter()
        .map(|item| {
            let cols = item.columns(resource_type);
            let cells: Vec<Cell> = cols
                .into_iter()
                .enumerate()
                .map(|(i, val)| {
                    let style = if i == 1 {
                        status_style(&val)
                    } else {
                        Style::default()
                    };
                    Cell::from(val).style(style)
                })
                .collect();
            Row::new(cells).height(1)
        })
        .collect();

    let widths = column_widths(resource_type);

    let title = if app.filter.is_empty() {
        format!(" {} ", resource_type)
    } else {
        format!(" {} [filter: {}] ", resource_type, app.filter)
    };

    let highlight_style = Style::default()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    let border_style = if app.focus == crate::types::Focus::ResourceList {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let table = Table::new(rows, &widths)
        .header(header_row)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .row_highlight_style(highlight_style)
        .highlight_symbol("▶ ");

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn column_widths(resource_type: crate::types::ResourceType) -> Vec<ratatui::layout::Constraint> {
    use crate::types::ResourceType;
    use ratatui::layout::Constraint;

    match resource_type {
        ResourceType::Pods => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
        ResourceType::PersistentVolumeClaims => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
        ResourceType::StatefulSets => vec![
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
    }
}

fn status_style(status: &str) -> Style {
    match status {
        "Running" | "Bound" | "Active" => Style::default().fg(Color::Green),
        "Pending" | "ContainerCreating" => Style::default().fg(Color::Yellow),
        "Failed" | "Error" | "CrashLoopBackOff" | "Lost" => Style::default().fg(Color::Red),
        "Terminating" => Style::default().fg(Color::Magenta),
        "Succeeded" | "Completed" => Style::default().fg(Color::Blue),
        _ => Style::default(),
    }
}
