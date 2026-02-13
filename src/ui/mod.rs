pub mod detail;
pub mod header;
pub mod help;
pub mod logs;
pub mod resource_list;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::App;
use crate::types::ViewMode;

pub fn render(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header selectors
            Constraint::Min(10),  // main content
            Constraint::Length(1), // footer keybindings
        ])
        .split(frame.area());

    header::render(frame, app, chunks[0]);

    match app.view_mode {
        ViewMode::List => {
            resource_list::render(frame, app, chunks[1]);
        }
        ViewMode::Detail | ViewMode::Confirm(_) => {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(chunks[1]);
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
                .split(chunks[1]);
            resource_list::render(frame, app, split[0]);
            logs::render(frame, app, split[1]);
        }
    }

    help::render_footer(frame, app, chunks[2]);
}
