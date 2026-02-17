pub mod detail;
pub mod header;
pub mod help;
pub mod logs;
pub mod resource_list;
pub mod search;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::App;
use crate::types::ViewMode;

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

    let dropdown_height: u16 = if app.dropdown_visible {
        // Show up to 10 items + 2 for border
        let item_count = app.dropdown_filtered.len() as u16;
        (item_count + 2).min(12).max(3)
    } else {
        0
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),               // header selectors
            Constraint::Length(dropdown_height),  // dropdown (0 when hidden)
            Constraint::Min(10),                 // main content
            Constraint::Length(1),               // footer keybindings
        ])
        .split(frame.area());

    header::render(frame, app, chunks[0]);

    if app.dropdown_visible {
        header::render_dropdown(frame, app, chunks[1]);
    }

    match app.view_mode {
        ViewMode::List => {
            resource_list::render(frame, app, chunks[2]);
        }
        ViewMode::Detail | ViewMode::Confirm(_) => {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(chunks[2]);
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
                .split(chunks[2]);
            resource_list::render(frame, app, split[0]);
            logs::render(frame, app, split[1]);
        }
        ViewMode::Search => unreachable!(), // handled above
    }

    help::render_footer(frame, app, chunks[3]);
}
