use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::types::{
    fuzzy_match, ConfirmAction, Focus, ResourceItem, ResourceType, SearchResult, ViewMode,
};

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

    // Dropdown selector
    pub dropdown_query: String,
    pub dropdown_filtered: Vec<usize>, // indices into the items list for the focused selector
    pub dropdown_selected: usize,      // index into dropdown_filtered

    // Search
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_filtered: Vec<usize>,
    pub search_table_state: TableState,
    pub search_loading: bool,
    pub search_contexts_total: usize,
    pub search_contexts_done: usize,
    pub entered_from_search: bool,

    // Quit
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));

        let mut app = Self {
            contexts: vec!["default-context".to_string()],
            selected_context: 0,
            namespaces: vec!["default".to_string()],
            selected_namespace: 0,
            resource_type: ResourceType::Pods,
            focus: Focus::ContextSelector,

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

            dropdown_query: String::new(),
            dropdown_filtered: Vec::new(),
            dropdown_selected: 0,

            search_query: String::new(),
            search_results: Vec::new(),
            search_filtered: Vec::new(),
            search_table_state: TableState::default(),
            search_loading: false,
            search_contexts_total: 0,
            search_contexts_done: 0,
            entered_from_search: false,

            should_quit: false,
        };
        app.dropdown_open();
        app
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

    pub fn selected_search_result(&self) -> Option<&SearchResult> {
        let idx = self.search_table_state.selected()?;
        let &filtered_idx = self.search_filtered.get(idx)?;
        self.search_results.get(filtered_idx)
    }

    pub fn update_search_filter(&mut self) {
        if self.search_query.is_empty() {
            self.search_filtered = (0..self.search_results.len()).collect();
        } else {
            let mut scored: Vec<(usize, i64)> = self
                .search_results
                .iter()
                .enumerate()
                .filter_map(|(i, r)| {
                    fuzzy_match(&self.search_query, &r.resource.name).map(|score| (i, score))
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.search_filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
        // Reset selection to top
        if self.search_filtered.is_empty() {
            self.search_table_state.select(None);
        } else {
            self.search_table_state.select(Some(0));
        }
    }

    /// Returns the list of items for the currently focused selector.
    pub fn dropdown_items(&self) -> Vec<String> {
        match self.focus {
            Focus::ContextSelector => self.contexts.clone(),
            Focus::NamespaceSelector => self.namespaces.clone(),
            Focus::ResourceTypeSelector => {
                ResourceType::ALL.iter().map(|t| t.to_string()).collect()
            }
            Focus::ResourceList => Vec::new(),
        }
    }

    /// Initialize dropdown state when entering a selector.
    pub fn dropdown_open(&mut self) {
        self.dropdown_query.clear();
        self.update_dropdown_filter();
    }

    /// Re-filter the dropdown items using fuzzy match on the query.
    pub fn update_dropdown_filter(&mut self) {
        let items = self.dropdown_items();
        if self.dropdown_query.is_empty() {
            self.dropdown_filtered = (0..items.len()).collect();
        } else {
            let mut scored: Vec<(usize, i64)> = items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    fuzzy_match(&self.dropdown_query, item).map(|score| (i, score))
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.dropdown_filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
        // Reset selection to top or clamp
        if self.dropdown_filtered.is_empty() {
            self.dropdown_selected = 0;
        } else {
            self.dropdown_selected = self.dropdown_selected.min(self.dropdown_filtered.len() - 1);
        }
    }

    /// Confirm the currently selected dropdown item.
    /// Returns the InputAction if a selection was made (and advances focus).
    fn dropdown_confirm(&mut self) -> InputAction {
        if let Some(&item_idx) = self.dropdown_filtered.get(self.dropdown_selected) {
            let action = match self.focus {
                Focus::ContextSelector => {
                    if item_idx != self.selected_context {
                        self.selected_context = item_idx;
                        InputAction::ContextChanged
                    } else {
                        InputAction::None
                    }
                }
                Focus::NamespaceSelector => {
                    if item_idx != self.selected_namespace {
                        self.selected_namespace = item_idx;
                        InputAction::NamespaceChanged
                    } else {
                        InputAction::None
                    }
                }
                Focus::ResourceTypeSelector => {
                    let new_type = ResourceType::ALL[item_idx];
                    if new_type != self.resource_type {
                        self.resource_type = new_type;
                        InputAction::ResourceTypeChanged
                    } else {
                        InputAction::None
                    }
                }
                Focus::ResourceList => InputAction::None,
            };
            // Advance focus to next selector
            self.focus = self.focus.next();
            if matches!(
                self.focus,
                Focus::ContextSelector | Focus::NamespaceSelector | Focus::ResourceTypeSelector
            ) {
                self.dropdown_open();
            }
            action
        } else {
            // No selection available, just advance
            self.focus = self.focus.next();
            if matches!(
                self.focus,
                Focus::ContextSelector | Focus::NamespaceSelector | Focus::ResourceTypeSelector
            ) {
                self.dropdown_open();
            }
            InputAction::None
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

        // Global Ctrl+F to enter search (from List or selector views, not from other modes)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
            if self.view_mode == ViewMode::List {
                self.view_mode = ViewMode::Search;
                self.search_query.clear();
                self.search_results.clear();
                self.search_filtered.clear();
                self.search_table_state.select(None);
                self.search_loading = true;
                self.search_contexts_done = 0;
                self.entered_from_search = false;
                return InputAction::StartSearch;
            }
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
            ViewMode::Detail if self.entered_from_search => self.handle_search_detail_input(key),
            ViewMode::Detail => self.handle_detail_input(key),
            ViewMode::Logs if self.entered_from_search => self.handle_search_logs_input(key),
            ViewMode::Logs => self.handle_logs_input(key),
            ViewMode::Confirm(_) => unreachable!(),
            ViewMode::Search => self.handle_search_input(key),
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
            Focus::ContextSelector
            | Focus::NamespaceSelector
            | Focus::ResourceTypeSelector => self.handle_selector_input(key),
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
                if matches!(
                    self.focus,
                    Focus::ContextSelector
                        | Focus::NamespaceSelector
                        | Focus::ResourceTypeSelector
                ) {
                    self.dropdown_open();
                }
                InputAction::None
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                if matches!(
                    self.focus,
                    Focus::ContextSelector
                        | Focus::NamespaceSelector
                        | Focus::ResourceTypeSelector
                ) {
                    self.dropdown_open();
                }
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

    fn handle_selector_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::ResourceList;
                InputAction::None
            }
            KeyCode::Enter | KeyCode::Tab => self.dropdown_confirm(),
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                if matches!(
                    self.focus,
                    Focus::ContextSelector
                        | Focus::NamespaceSelector
                        | Focus::ResourceTypeSelector
                ) {
                    self.dropdown_open();
                }
                InputAction::None
            }
            KeyCode::Down => {
                if !self.dropdown_filtered.is_empty() {
                    self.dropdown_selected =
                        (self.dropdown_selected + 1) % self.dropdown_filtered.len();
                }
                InputAction::None
            }
            KeyCode::Up => {
                if !self.dropdown_filtered.is_empty() {
                    self.dropdown_selected = if self.dropdown_selected == 0 {
                        self.dropdown_filtered.len() - 1
                    } else {
                        self.dropdown_selected - 1
                    };
                }
                InputAction::None
            }
            KeyCode::Backspace => {
                self.dropdown_query.pop();
                self.dropdown_selected = 0;
                self.update_dropdown_filter();
                InputAction::None
            }
            KeyCode::Char(c) => {
                self.dropdown_query.push(c);
                self.dropdown_selected = 0;
                self.update_dropdown_filter();
                InputAction::None
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
            KeyCode::Char('o') => InputAction::OpenLogsInEditor,
            KeyCode::Char('O') => InputAction::OpenLogsInLess,
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

    fn handle_search_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Esc => {
                self.view_mode = ViewMode::List;
                self.entered_from_search = false;
                InputAction::None
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.update_search_filter();
                InputAction::None
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.update_search_filter();
                InputAction::None
            }
            KeyCode::Down | KeyCode::Tab => {
                self.search_select_next();
                InputAction::None
            }
            KeyCode::Up | KeyCode::BackTab => {
                self.search_select_prev();
                InputAction::None
            }
            KeyCode::Enter => {
                if self.selected_search_result().is_some() {
                    self.view_mode = ViewMode::Detail;
                    self.entered_from_search = true;
                    self.detail_scroll = 0;
                    self.detail_text.clear();
                    InputAction::SearchDescribe
                } else {
                    InputAction::None
                }
            }
            _ => InputAction::None,
        }
    }

    fn handle_search_detail_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.view_mode = ViewMode::Search;
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
                let lines = self.detail_text.lines().count() as u16;
                self.detail_scroll = lines.saturating_sub(10);
                InputAction::None
            }
            KeyCode::Char('g') => {
                self.detail_scroll = 0;
                InputAction::None
            }
            KeyCode::Char('l') => {
                if let Some(result) = self.selected_search_result() {
                    if result.resource_type == ResourceType::Pods {
                        self.view_mode = ViewMode::Logs;
                        self.log_lines.clear();
                        self.log_scroll = 0;
                        self.log_follow = true;
                        InputAction::SearchStreamLogs
                    } else {
                        InputAction::None
                    }
                } else {
                    InputAction::None
                }
            }
            _ => InputAction::None,
        }
    }

    fn handle_search_logs_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.view_mode = ViewMode::Search;
                InputAction::StopLogs
            }
            KeyCode::Char('f') => {
                self.log_follow = !self.log_follow;
                InputAction::None
            }
            KeyCode::Char('o') => InputAction::OpenLogsInEditor,
            KeyCode::Char('O') => InputAction::OpenLogsInLess,
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

    fn search_select_next(&mut self) {
        let len = self.search_filtered.len();
        if len == 0 {
            return;
        }
        let i = self
            .search_table_state
            .selected()
            .map(|i| (i + 1) % len)
            .unwrap_or(0);
        self.search_table_state.select(Some(i));
    }

    fn search_select_prev(&mut self) {
        let len = self.search_filtered.len();
        if len == 0 {
            return;
        }
        let i = self
            .search_table_state
            .selected()
            .map(|i| if i == 0 { len - 1 } else { i - 1 })
            .unwrap_or(0);
        self.search_table_state.select(Some(i));
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
    OpenLogsInEditor,
    OpenLogsInLess,
    StartSearch,
    SearchDescribe,
    SearchStreamLogs,
}
