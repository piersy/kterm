use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::app::{App, DisplayRow};
use crate::types::ResourceType;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let display_rows = app.display_rows();
    let multi_type = app.selected_resource_types.len() > 1;

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
    let headers = resource_type.column_headers();

    let header_cells: Vec<Cell> = headers
        .iter()
        .map(|h| {
            Cell::from(*h).style(
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
    // We use a NAME + STATUS + AGE layout for mixed types.
    let generic_headers = vec!["TYPE", "NAME", "STATUS", "AGE"];
    let header_cells: Vec<Cell> = generic_headers
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    let header_row = Row::new(header_cells).height(1);

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
                Row::new(vec![cell, Cell::from(""), Cell::from(""), Cell::from("")])
                    .height(1)
                    .style(Style::default().fg(Color::DarkGray))
            }
            DisplayRow::Resource {
                resource_type,
                index,
            } => {
                let items = app.resources_by_type.get(resource_type);
                if let Some(item) = items.and_then(|v| v.get(*index)) {
                    let cells = vec![
                        Cell::from(resource_type.to_string())
                            .style(Style::default().fg(Color::DarkGray)),
                        Cell::from(item.name.clone()),
                        Cell::from(item.status.clone()).style(status_style(&item.status)),
                        Cell::from(item.age.clone()),
                    ];
                    Row::new(cells).height(1)
                } else {
                    Row::new(vec![Cell::from("?")]).height(1)
                }
            }
        })
        .collect();

    let widths = vec![
        Constraint::Percentage(15),
        Constraint::Percentage(35),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ];

    let types_display: String = app
        .selected_resource_types
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let title = if app.filter.is_empty() {
        format!(" {} ", types_display)
    } else {
        format!(" {} [filter: {}] ", types_display, app.filter)
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

fn column_widths(resource_type: ResourceType) -> Vec<Constraint> {
    match resource_type {
        ResourceType::Pods => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
        ResourceType::Deployments => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
        ],
        ResourceType::StatefulSets => vec![
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
        ResourceType::DaemonSets
        | ResourceType::ReplicaSets
        | ResourceType::ReplicationControllers => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
        ResourceType::Jobs => vec![
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
        ResourceType::CronJobs => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
        ResourceType::HorizontalPodAutoscalers => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
        ResourceType::Services => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
        ],
        ResourceType::Endpoints => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(50),
            Constraint::Percentage(20),
        ],
        ResourceType::Ingresses => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(20),
            Constraint::Percentage(35),
            Constraint::Percentage(20),
        ],
        ResourceType::NetworkPolicies => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(50),
            Constraint::Percentage(20),
        ],
        ResourceType::ConfigMaps => vec![
            Constraint::Percentage(50),
            Constraint::Percentage(20),
            Constraint::Percentage(30),
        ],
        ResourceType::Secrets => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(30),
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
        ResourceType::PersistentVolumes => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
        ],
        ResourceType::StorageClasses => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(50),
            Constraint::Percentage(20),
        ],
        ResourceType::ServiceAccounts
        | ResourceType::ResourceQuotas
        | ResourceType::LimitRanges => vec![Constraint::Percentage(60), Constraint::Percentage(40)],
        ResourceType::Namespaces => vec![
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
        ResourceType::Nodes => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
        ResourceType::Events => vec![
            Constraint::Percentage(20),
            Constraint::Percentage(10),
            Constraint::Percentage(15),
            Constraint::Percentage(40),
            Constraint::Percentage(15),
        ],
        ResourceType::PodDisruptionBudgets => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
        ],
    }
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
