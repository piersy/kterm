use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::types::{ConfirmAction, Focus, ResourceItem, ResourceType, ViewMode};

pub struct App {
    // Navigation
    pub contexts: Vec<String>,
    pub selected_context: usize,
    pub namespaces: Vec<String>,
    pub selected_namespace: usize,
    pub resource_type: ResourceType,
    pub focus: Focus,

    // Resource list
    pub resources: Vec<ResourceItem>,
    pub table_state: TableState,
    pub loading: bool,

    // Detail view
    pub detail_text: String,
    pub detail_scroll: u16,

    // Logs view
    pub log_lines: Vec<String>,
    pub log_scroll: u16,
    pub log_follow: bool,

    // Mode
    pub view_mode: ViewMode,

    // Filter
    pub filter: String,
    pub filter_active: bool,

    // Error
    pub error_message: Option<String>,
    pub error_ticks: u8,

    // Quit
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));

        Self {
            contexts: vec!["default-context".to_string()],
            selected_context: 0,
            namespaces: vec!["default".to_string()],
            selected_namespace: 0,
            resource_type: ResourceType::Pods,
            focus: Focus::ResourceList,

            resources: Vec::new(),
            table_state,
            loading: false,

            detail_text: String::new(),
            detail_scroll: 0,

            log_lines: Vec::new(),
            log_scroll: 0,
            log_follow: true,

            view_mode: ViewMode::List,

            filter: String::new(),
            filter_active: false,

            error_message: None,
            error_ticks: 0,

            should_quit: false,
        }
    }

    pub fn current_context(&self) -> &str {
        self.contexts
            .get(self.selected_context)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn current_namespace(&self) -> &str {
        self.namespaces
            .get(self.selected_namespace)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn selected_resource(&self) -> Option<&ResourceItem> {
        let idx = self.table_state.selected()?;
        self.filtered_resources().into_iter().nth(idx)
    }

    pub fn selected_resource_name(&self) -> Option<String> {
        self.selected_resource().map(|r| r.name.clone())
    }

    pub fn filtered_resources(&self) -> Vec<&ResourceItem> {
        if self.filter.is_empty() {
            self.resources.iter().collect()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.resources
                .iter()
                .filter(|r| r.name.to_lowercase().contains(&filter_lower))
                .collect()
        }
    }

    pub fn handle_tick(&mut self) {
        if let Some(ref _msg) = self.error_message {
            self.error_ticks += 1;
            if self.error_ticks > 20 {
                // ~5 seconds at 250ms tick
                self.error_message = None;
                self.error_ticks = 0;
            }
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
        self.error_ticks = 0;
    }

    /// Handle key input. Returns true if an action requiring K8s interaction was triggered.
    pub fn handle_input(&mut self, key: KeyEvent) -> InputAction {
        // Global quit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return InputAction::None;
        }

        // Filter mode input
        if self.filter_active {
            return self.handle_filter_input(key);
        }

        // Confirmation dialog
        if let ViewMode::Confirm(action) = self.view_mode {
            return self.handle_confirm_input(key, action);
        }

        match self.view_mode {
            ViewMode::List => self.handle_list_input(key),
            ViewMode::Detail => self.handle_detail_input(key),
            ViewMode::Logs => self.handle_logs_input(key),
            ViewMode::Confirm(_) => unreachable!(),
        }
    }

    fn handle_filter_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Esc => {
                self.filter_active = false;
            }
            KeyCode::Enter => {
                self.filter_active = false;
                // Keep the filter but exit filter mode
                self.table_state.select(Some(0));
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.table_state.select(Some(0));
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.table_state.select(Some(0));
            }
            _ => {}
        }
        InputAction::None
    }

    fn handle_confirm_input(&mut self, key: KeyEvent, action: ConfirmAction) -> InputAction {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.view_mode = ViewMode::List;
                match action {
                    ConfirmAction::Delete => InputAction::Delete,
                    ConfirmAction::Restart => InputAction::Restart,
                }
            }
            _ => {
                // Any other key cancels
                self.view_mode = ViewMode::List;
                InputAction::None
            }
        }
    }

    fn handle_list_input(&mut self, key: KeyEvent) -> InputAction {
        match self.focus {
            Focus::ResourceList => self.handle_resource_list_input(key),
            Focus::ContextSelector => self.handle_context_selector_input(key),
            Focus::NamespaceSelector => self.handle_namespace_selector_input(key),
            Focus::ResourceTypeSelector => self.handle_resource_type_selector_input(key),
        }
    }

    fn handle_resource_list_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                InputAction::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.select_next();
                InputAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.select_prev();
                InputAction::None
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                InputAction::None
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                InputAction::None
            }
            KeyCode::Enter => {
                if self.selected_resource().is_some() {
                    self.view_mode = ViewMode::Detail;
                    self.detail_scroll = 0;
                    InputAction::Describe
                } else {
                    InputAction::None
                }
            }
            KeyCode::Char('l') => {
                if self.resource_type == ResourceType::Pods && self.selected_resource().is_some() {
                    self.view_mode = ViewMode::Logs;
                    self.log_lines.clear();
                    self.log_scroll = 0;
                    self.log_follow = true;
                    InputAction::StreamLogs
                } else {
                    InputAction::None
                }
            }
            KeyCode::Char('d') => {
                if self.selected_resource().is_some() {
                    self.view_mode = ViewMode::Confirm(ConfirmAction::Delete);
                }
                InputAction::None
            }
            KeyCode::Char('r') => {
                if self.selected_resource().is_some() {
                    self.view_mode = ViewMode::Confirm(ConfirmAction::Restart);
                }
                InputAction::None
            }
            KeyCode::Char('e') => {
                if self.selected_resource().is_some() {
                    InputAction::Edit
                } else {
                    InputAction::None
                }
            }
            KeyCode::Char('/') => {
                self.filter_active = true;
                self.filter.clear();
                InputAction::None
            }
            KeyCode::Char('?') => {
                // TODO: help overlay
                InputAction::None
            }
            _ => InputAction::None,
        }
    }

    fn handle_context_selector_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                InputAction::None
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                InputAction::None
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                InputAction::None
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if !self.contexts.is_empty() {
                    self.selected_context = (self.selected_context + 1) % self.contexts.len();
                    InputAction::ContextChanged
                } else {
                    InputAction::None
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if !self.contexts.is_empty() {
                    self.selected_context = if self.selected_context == 0 {
                        self.contexts.len() - 1
                    } else {
                        self.selected_context - 1
                    };
                    InputAction::ContextChanged
                } else {
                    InputAction::None
                }
            }
            _ => InputAction::None,
        }
    }

    fn handle_namespace_selector_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                InputAction::None
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                InputAction::None
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                InputAction::None
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if !self.namespaces.is_empty() {
                    self.selected_namespace =
                        (self.selected_namespace + 1) % self.namespaces.len();
                    InputAction::NamespaceChanged
                } else {
                    InputAction::None
                }
            }
            KeyCode::Char('h') | KeyCode::Left => {
                if !self.namespaces.is_empty() {
                    self.selected_namespace = if self.selected_namespace == 0 {
                        self.namespaces.len() - 1
                    } else {
                        self.selected_namespace - 1
                    };
                    InputAction::NamespaceChanged
                } else {
                    InputAction::None
                }
            }
            _ => InputAction::None,
        }
    }

    fn handle_resource_type_selector_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                InputAction::None
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                InputAction::None
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                InputAction::None
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.resource_type = self.resource_type.next();
                InputAction::ResourceTypeChanged
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.resource_type = self.resource_type.prev();
                InputAction::ResourceTypeChanged
            }
            _ => InputAction::None,
        }
    }

    fn handle_detail_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.view_mode = ViewMode::List;
                InputAction::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.detail_scroll = self.detail_scroll.saturating_add(1);
                InputAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.detail_scroll = self.detail_scroll.saturating_sub(1);
                InputAction::None
            }
            KeyCode::Char('G') => {
                // Jump to bottom
                let lines = self.detail_text.lines().count() as u16;
                self.detail_scroll = lines.saturating_sub(10);
                InputAction::None
            }
            KeyCode::Char('g') => {
                self.detail_scroll = 0;
                InputAction::None
            }
            KeyCode::Char('l') => {
                if self.resource_type == ResourceType::Pods && self.selected_resource().is_some() {
                    self.view_mode = ViewMode::Logs;
                    self.log_lines.clear();
                    self.log_scroll = 0;
                    self.log_follow = true;
                    InputAction::StreamLogs
                } else {
                    InputAction::None
                }
            }
            KeyCode::Char('d') => {
                if self.selected_resource().is_some() {
                    self.view_mode = ViewMode::Confirm(ConfirmAction::Delete);
                }
                InputAction::None
            }
            KeyCode::Char('r') => {
                if self.selected_resource().is_some() {
                    self.view_mode = ViewMode::Confirm(ConfirmAction::Restart);
                }
                InputAction::None
            }
            KeyCode::Char('e') => {
                if self.selected_resource().is_some() {
                    InputAction::Edit
                } else {
                    InputAction::None
                }
            }
            _ => InputAction::None,
        }
    }

    fn handle_logs_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.view_mode = ViewMode::List;
                InputAction::StopLogs
            }
            KeyCode::Char('f') => {
                self.log_follow = !self.log_follow;
                InputAction::None
            }
            KeyCode::Char('G') => {
                let lines = self.log_lines.len() as u16;
                self.log_scroll = lines.saturating_sub(10);
                self.log_follow = true;
                InputAction::None
            }
            KeyCode::Char('g') => {
                self.log_scroll = 0;
                self.log_follow = false;
                InputAction::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.log_scroll = self.log_scroll.saturating_add(1);
                self.log_follow = false;
                InputAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.log_scroll = self.log_scroll.saturating_sub(1);
                self.log_follow = false;
                InputAction::None
            }
            _ => InputAction::None,
        }
    }

    fn select_next(&mut self) {
        let len = self.filtered_resources().len();
        if len == 0 {
            return;
        }
        let i = self
            .table_state
            .selected()
            .map(|i| (i + 1) % len)
            .unwrap_or(0);
        self.table_state.select(Some(i));
    }

    fn select_prev(&mut self) {
        let len = self.filtered_resources().len();
        if len == 0 {
            return;
        }
        let i = self
            .table_state
            .selected()
            .map(|i| if i == 0 { len - 1 } else { i - 1 })
            .unwrap_or(0);
        self.table_state.select(Some(i));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    None,
    ContextChanged,
    NamespaceChanged,
    ResourceTypeChanged,
    Describe,
    StreamLogs,
    StopLogs,
    Delete,
    Restart,
    Edit,
}
