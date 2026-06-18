use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::app::{App, DisplayRow};
use crate::types::{is_all_namespaces, ColumnDef};

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let display_rows = app.display_rows();
    // The related-components view always uses the multi-type layout so its
    // results are grouped under per-type dividers.
    let multi_type =
        app.view_mode == crate::types::ViewMode::Related || app.selected_resource_types.len() > 1;

    if !multi_type {
        // Single type: use original table rendering
        render_single_type(frame, app, area);
    } else {
        // Multi-type: render with divider lines
        render_multi_type(frame, app, area, &display_rows);
    }
}

fn render_single_type(frame: &mut Frame, app: &mut App, area: Rect) {
    let resource_type = app.primary_resource_type();
    let all_ns = is_all_namespaces(app.current_namespace());
    let defs = resource_type.column_defs(all_ns);

    let header_cells: Vec<Cell> = defs
        .iter()
        .map(|d| {
            Cell::from(d.header).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    let header_row = Row::new(header_cells).height(1);

    let items = app
        .resources_by_type
        .get(&resource_type)
        .cloned()
        .unwrap_or_default();
    let filtered: Vec<_> = if app.filter.is_empty() {
        items.iter().collect()
    } else {
        let filter_lower = app.filter.to_lowercase();
        items
            .iter()
            .filter(|r| r.name.to_lowercase().contains(&filter_lower))
            .collect()
    };

    let rows: Vec<Row> = filtered
        .iter()
        .map(|item| {
            let cols = item.column_values(&defs);
            let cells: Vec<Cell> = cols
                .into_iter()
                .enumerate()
                .map(|(i, val)| {
                    let style = if defs.get(i).is_some_and(|d| d.is_status) {
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

    let widths = ColumnDef::to_constraints(&defs);

    let title = if app.filter.is_empty() {
        format!(" {} ", resource_type)
    } else {
        format!(" {} [filter: {}] ", resource_type, app.filter)
    };

    let highlight_style = Style::default()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    let border_style = Style::default().fg(Color::Cyan);

    let table = Table::new(rows, &widths)
        .header(header_row)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .row_highlight_style(highlight_style)
        .highlight_symbol("\u{25b6} ");

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn render_multi_type(frame: &mut Frame, app: &mut App, area: Rect, display_rows: &[DisplayRow]) {
    // For multi-type display, we use a single table with variable-width columns.
    // Divider rows span the full width. Resource rows use a generic column layout.
    let related = app.view_mode == crate::types::ViewMode::Related;
    let all_ns = if related {
        is_all_namespaces(&app.related_namespace)
    } else {
        is_all_namespaces(app.current_namespace())
    };
    let defs = multi_type_column_defs(all_ns);

    let header_cells: Vec<Cell> = defs
        .iter()
        .map(|d| {
            Cell::from(d.header).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    let header_row = Row::new(header_cells).height(1);

    let n_cols = defs.len();

    let rows: Vec<Row> = display_rows
        .iter()
        .map(|row| match row {
            DisplayRow::TypeDivider(rt) => {
                // Create a divider row with the type name
                let divider_text = format!("\u{2500}\u{2500} {} \u{2500}\u{2500}", rt);
                let cell = Cell::from(divider_text).style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
                let mut cells = vec![cell];
                for _ in 1..n_cols {
                    cells.push(Cell::from(""));
                }
                Row::new(cells)
                    .height(1)
                    .style(Style::default().fg(Color::DarkGray))
            }
            DisplayRow::Resource {
                resource_type,
                index,
            } => {
                if let Some(item) = app.row_item(*resource_type, *index) {
                    let mut cells = vec![
                        Cell::from(resource_type.to_string())
                            .style(Style::default().fg(Color::DarkGray)),
                        Cell::from(item.name.clone()),
                    ];
                    if all_ns {
                        cells.push(Cell::from(item.namespace.clone()));
                    }
                    cells.push(
                        Cell::from(item.status.clone()).style(status_style(&item.status)),
                    );
                    cells.push(Cell::from(item.age()));
                    Row::new(cells).height(1)
                } else {
                    Row::new(vec![Cell::from("?")]).height(1)
                }
            }
        })
        .collect();

    let widths = ColumnDef::to_constraints(&defs);

    let title = if related {
        let status = if app.related_loading {
            " — loading…"
        } else if display_rows.is_empty() {
            " — none found"
        } else {
            ""
        };
        let ns = if is_all_namespaces(&app.related_namespace) {
            "all namespaces".to_string()
        } else {
            app.related_namespace.clone()
        };
        format!(
            " Related: {}={} in {}{} ",
            app.related_label, app.related_label_value, ns, status
        )
    } else {
        let types_display: String = app
            .selected_resource_types
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        if app.filter.is_empty() {
            format!(" {} ", types_display)
        } else {
            format!(" {} [filter: {}] ", types_display, app.filter)
        }
    };

    let highlight_style = Style::default()
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    let border_style = Style::default().fg(Color::Cyan);

    let table = Table::new(rows, &widths)
        .header(header_row)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .row_highlight_style(highlight_style)
        .highlight_symbol("\u{25b6} ");

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

/// Column definitions for the generic multi-type table.
fn multi_type_column_defs(all_namespaces: bool) -> Vec<ColumnDef> {
    let mut defs = vec![
        ColumnDef::col("TYPE", 15),
        ColumnDef::name(35),
    ];
    if all_namespaces {
        defs.push(ColumnDef::col("NAMESPACE", 15));
    }
    defs.push(ColumnDef { header: "STATUS", width: 25, is_status: true });
    defs.push(ColumnDef::col("AGE", 25));
    defs
}

fn status_style(status: &str) -> Style {
    match status {
        "Running" | "Bound" | "Active" | "Ready" | "Available" => {
            Style::default().fg(Color::Green)
        }
        "Pending" | "ContainerCreating" | "Updating" => Style::default().fg(Color::Yellow),
        "Failed" | "Error" | "CrashLoopBackOff" | "Lost" | "NotReady" => {
            Style::default().fg(Color::Red)
        }
        "Terminating" => Style::default().fg(Color::Magenta),
        "Succeeded" | "Completed" | "Released" => Style::default().fg(Color::Blue),
        _ => Style::default(),
    }
}
