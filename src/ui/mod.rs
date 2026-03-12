pub mod detail;
pub mod header;
pub mod help;
pub mod logs;
pub mod resource_list;
pub mod search;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::widgets::Clear;
use ratatui::Frame;

use crate::app::App;
use crate::types::{Focus, ViewMode};

pub fn render(frame: &mut Frame, app: &mut App) {
    // Search mode takes over the full screen (no header selectors)
    if app.view_mode == ViewMode::Search {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),  // search content
                Constraint::Length(1), // footer
            ])
            .split(frame.area());
        search::render(frame, app, chunks[0]);
        help::render_footer(frame, app, chunks[1]);
        return;
    }

    // Detail/Logs entered from search: full-screen detail/logs with footer
    if app.entered_from_search && matches!(app.view_mode, ViewMode::Detail | ViewMode::Logs) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),  // content
                Constraint::Length(1), // footer
            ])
            .split(frame.area());

        match app.view_mode {
            ViewMode::Detail => detail::render(frame, app, chunks[0]),
            ViewMode::Logs => logs::render(frame, app, chunks[0]),
            _ => unreachable!(),
        }
        help::render_footer(frame, app, chunks[1]);
        return;
    }

    // Normal layout: 3 stacked selector rows + main content + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header: 3 stacked selector rows (1 line each)
            Constraint::Min(10),  // main content
            Constraint::Length(1), // footer keybindings
        ])
        .split(frame.area());

    header::render(frame, app, chunks[0]);

    // Render main content
    let content_area = chunks[1];
    match app.view_mode {
        ViewMode::List => {
            resource_list::render(frame, app, content_area);
        }
        ViewMode::Detail | ViewMode::Confirm(_) => {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(content_area);
            resource_list::render(frame, app, split[0]);
            detail::render(frame, app, split[1]);

            if let ViewMode::Confirm(action) = app.view_mode {
                help::render_confirm_dialog(frame, action);
            }
        }
        ViewMode::Logs => {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(content_area);
            resource_list::render(frame, app, split[0]);
            logs::render(frame, app, split[1]);
        }
        ViewMode::Search => unreachable!(), // handled above
    }

    // Render dropdown overlay on top of content if a selector is active
    if matches!(app.focus, Focus::Selector(_)) && app.dropdown_visible {
        let dropdown_height = {
            let item_count = app.dropdown_filtered.len() as u16;
            (item_count + 2).clamp(3, 12)
        };

        // Position the dropdown right below the header area, overlaying content
        let dropdown_area = Rect {
            x: chunks[0].x,
            y: chunks[0].y + chunks[0].height,
            width: chunks[0].width,
            height: dropdown_height.min(content_area.height),
        };

        // Clear the area first so it acts as an overlay
        frame.render_widget(Clear, dropdown_area);
        header::render_dropdown(frame, app, dropdown_area);
    }

    help::render_footer(frame, app, chunks[2]);
}
