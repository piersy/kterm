use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search input
            Constraint::Min(5),   // results table
        ])
        .split(area);

    render_search_input(frame, app, chunks[0]);
    render_search_results(frame, app, chunks[1]);
}

fn render_search_input(frame: &mut Frame, app: &App, area: Rect) {
    let display_text = format!("{}\u{2588}", app.search_query); // block cursor

    let block = Block::default()
        .title(" Search (Ctrl+F) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let paragraph = Paragraph::new(display_text)
        .block(block)
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);
}

fn render_search_results(frame: &mut Frame, app: &mut App, area: Rect) {
    let header_cells = ["NAME", "TYPE", "NAMESPACE", "CLUSTER"]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header_row = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .search_filtered
        .iter()
        .filter_map(|&idx| app.search_results.get(idx))
        .map(|result| {
            Row::new(vec![
                Cell::from(result.resource.name.clone()),
                Cell::from(result.resource_type.to_string()),
                Cell::from(result.resource.namespace.clone()),
                Cell::from(result.context.clone()),
            ])
            .height(1)
        })
        .collect();

    let title = if app.search_loading {
        let done = app.search_contexts_done;
        let total = app.search_contexts_total;
        format!(
            " Results ({} found, scanning {}/{} clusters...) ",
            app.search_filtered.len(),
            done,
            total
        )
    } else {
        format!(" Results ({} found) ", app.search_filtered.len())
    };

    let highlight_style = Style::default()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    let table = Table::new(
        rows,
        &[
            Constraint::Percentage(35),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
    )
    .header(header_row)
    .block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    )
    .row_highlight_style(highlight_style)
    .highlight_symbol("▶ ");

    frame.render_stateful_widget(table, area, &mut app.search_table_state);
}
