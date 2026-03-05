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
        // 5 columns: NAME, STATUS, AGE, RESTARTS, NODE
        ResourceType::Pods => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
        // 5 columns: NAME, READY, UP-TO-DATE, AVAILABLE, AGE
        ResourceType::Deployments => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
        ],
        // 3 columns: NAME, READY, AGE
        ResourceType::StatefulSets => vec![
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
        // 5 columns: NAME, DESIRED, CURRENT, READY, AGE
        ResourceType::DaemonSets | ResourceType::ReplicaSets | ResourceType::ReplicationControllers => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
        // 3 columns: NAME, COMPLETIONS, AGE
        ResourceType::Jobs => vec![
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
        // 5 columns: NAME, SCHEDULE, SUSPEND, ACTIVE, AGE
        ResourceType::CronJobs => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
        // 5 columns: NAME, MINPODS, MAXPODS, REPLICAS, AGE
        ResourceType::HorizontalPodAutoscalers => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
        // 5 columns: NAME, TYPE, CLUSTER-IP, PORTS, AGE
        ResourceType::Services => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
        ],
        // 3 columns: NAME, ENDPOINTS, AGE
        ResourceType::Endpoints => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(50),
            Constraint::Percentage(20),
        ],
        // 4 columns: NAME, CLASS, HOSTS, AGE
        ResourceType::Ingresses => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(20),
            Constraint::Percentage(35),
            Constraint::Percentage(20),
        ],
        // 3 columns: NAME, POD-SELECTOR, AGE
        ResourceType::NetworkPolicies => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(50),
            Constraint::Percentage(20),
        ],
        // 3 columns: NAME, DATA, AGE
        ResourceType::ConfigMaps => vec![
            Constraint::Percentage(50),
            Constraint::Percentage(20),
            Constraint::Percentage(30),
        ],
        // 4 columns: NAME, TYPE, DATA, AGE
        ResourceType::Secrets => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(30),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
        ],
        // 5 columns: NAME, STATUS, VOLUME, CAPACITY, AGE
        ResourceType::PersistentVolumeClaims => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
        ],
        // 5 columns: NAME, CAPACITY, STATUS, STORAGECLASS, AGE
        ResourceType::PersistentVolumes => vec![
            Constraint::Percentage(25),
            Constraint::Percentage(15),
            Constraint::Percentage(15),
            Constraint::Percentage(25),
            Constraint::Percentage(20),
        ],
        // 3 columns: NAME, PROVISIONER, AGE
        ResourceType::StorageClasses => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(50),
            Constraint::Percentage(20),
        ],
        // 2 columns: NAME, AGE
        ResourceType::ServiceAccounts | ResourceType::ResourceQuotas | ResourceType::LimitRanges => vec![
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ],
        // 3 columns: NAME, STATUS, AGE
        ResourceType::Namespaces => vec![
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
        // 4 columns: NAME, STATUS, ROLES, AGE
        ResourceType::Nodes => vec![
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
        // 5 columns: NAME, TYPE, REASON, MESSAGE, AGE
        ResourceType::Events => vec![
            Constraint::Percentage(20),
            Constraint::Percentage(10),
            Constraint::Percentage(15),
            Constraint::Percentage(40),
            Constraint::Percentage(15),
        ],
        // 4 columns: NAME, MIN-AVAILABLE, MAX-UNAVAILABLE, AGE
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
        "Running" | "Bound" | "Active" | "Ready" | "Available" => Style::default().fg(Color::Green),
        "Pending" | "ContainerCreating" | "Updating" => Style::default().fg(Color::Yellow),
        "Failed" | "Error" | "CrashLoopBackOff" | "Lost" | "NotReady" => {
            Style::default().fg(Color::Red)
        }
        "Terminating" => Style::default().fg(Color::Magenta),
        "Succeeded" | "Completed" | "Released" => Style::default().fg(Color::Blue),
        _ => Style::default(),
    }
}
