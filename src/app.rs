use std::collections::{HashMap, HashSet};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::types::{
    fuzzy_match, ConfirmAction, Focus, ResourceItem, ResourceType, SearchResult, SelectorTarget,
    ViewMode,
};

pub struct App {
    // Navigation
    pub contexts: Vec<String>,
    pub selected_contexts: HashSet<usize>,
    pub namespaces: Vec<String>,
    pub selected_namespaces: HashSet<usize>,
    pub preferred_namespace: Option<String>,
    pub selected_resource_types: Vec<ResourceType>,
    pub focus: Focus,

    // Resource list (per-type storage for multi-type display)
    pub resources_by_type: HashMap<ResourceType, Vec<ResourceItem>>,
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
    pub dropdown_filtered: Vec<usize>,
    pub dropdown_selected: usize,
    pub dropdown_visible: bool,
    pub dropdown_toggled: HashSet<usize>, // items toggled with Space (multi-select)

    // Search
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_filtered: Vec<usize>,
    pub search_table_state: TableState,
    pub search_loading: bool,
    pub search_contexts_total: usize,
    pub search_contexts_done: usize,
    pub entered_from_search: bool,

    // Resource counts per type (for dropdown display)
    pub resource_counts: HashMap<ResourceType, usize>,

    // Quit
    pub should_quit: bool,
}

/// A row in the flattened multi-type resource list.
#[derive(Debug, Clone)]
pub enum DisplayRow {
    /// Divider line for a resource type section.
    TypeDivider(ResourceType),
    /// An actual resource row.
    Resource {
        resource_type: ResourceType,
        index: usize, // index into resources_by_type[resource_type]
    },
}

impl App {
    pub fn new() -> Self {
        let mut table_state = TableState::default();
        table_state.select(Some(0));

        Self {
            contexts: vec!["default-context".to_string()],
            selected_contexts: {
                let mut s = HashSet::new();
                s.insert(0);
                s
            },
            namespaces: vec!["default".to_string()],
            selected_namespaces: {
                let mut s = HashSet::new();
                s.insert(0);
                s
            },
            preferred_namespace: None,
            selected_resource_types: vec![ResourceType::Pods],
            focus: Focus::ResourceList,

            resources_by_type: HashMap::new(),
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
            dropdown_visible: false,
            dropdown_toggled: HashSet::new(),

            search_query: String::new(),
            search_results: Vec::new(),
            search_filtered: Vec::new(),
            search_table_state: TableState::default(),
            search_loading: false,
            search_contexts_total: 0,
            search_contexts_done: 0,
            entered_from_search: false,

            resource_counts: HashMap::new(),

            should_quit: false,
        }
    }

    /// Returns the first selected context name (primary context for K8s operations).
    pub fn current_context(&self) -> &str {
        self.selected_contexts
            .iter()
            .min()
            .and_then(|&idx| self.contexts.get(idx))
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Returns the first selected namespace name (primary namespace for K8s operations).
    pub fn current_namespace(&self) -> &str {
        self.selected_namespaces
            .iter()
            .min()
            .and_then(|&idx| self.namespaces.get(idx))
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    /// Returns the primary resource type (first selected).
    pub fn primary_resource_type(&self) -> ResourceType {
        self.selected_resource_types
            .first()
            .copied()
            .unwrap_or(ResourceType::Pods)
    }

    /// Build the flat list of display rows for multi-type view.
    pub fn display_rows(&self) -> Vec<DisplayRow> {
        let mut rows = Vec::new();
        let multi_type = self.selected_resource_types.len() > 1;

        for &rt in &self.selected_resource_types {
            let items = self.resources_by_type.get(&rt);
            if multi_type {
                rows.push(DisplayRow::TypeDivider(rt));
            }

            if let Some(items) = items {
                let filter_lower = self.filter.to_lowercase();
                for (i, item) in items.iter().enumerate() {
                    if self.filter.is_empty()
                        || item.name.to_lowercase().contains(&filter_lower)
                    {
                        rows.push(DisplayRow::Resource {
                            resource_type: rt,
                            index: i,
                        });
                    }
                }
            }
        }
        rows
    }

    /// Get the resource at the current table selection.
    pub fn selected_resource(&self) -> Option<(&ResourceItem, ResourceType)> {
        let idx = self.table_state.selected()?;
        let rows = self.display_rows();
        match rows.get(idx)? {
            DisplayRow::Resource {
                resource_type,
                index,
            } => {
                let item = self.resources_by_type.get(resource_type)?.get(*index)?;
                Some((item, *resource_type))
            }
            DisplayRow::TypeDivider(_) => None,
        }
    }

    pub fn selected_resource_name(&self) -> Option<String> {
        self.selected_resource().map(|(r, _)| r.name.clone())
    }

    /// Get the resource type of the currently selected row.
    pub fn selected_row_resource_type(&self) -> Option<ResourceType> {
        let idx = self.table_state.selected()?;
        let rows = self.display_rows();
        match rows.get(idx)? {
            DisplayRow::Resource { resource_type, .. } => Some(*resource_type),
            DisplayRow::TypeDivider(rt) => Some(*rt),
        }
    }

    #[allow(dead_code)]
    /// Legacy compatibility: flat list of all resources matching filter.
    pub fn filtered_resources(&self) -> Vec<&ResourceItem> {
        let rt = self.primary_resource_type();
        let items = match self.resources_by_type.get(&rt) {
            Some(items) => items,
            None => return Vec::new(),
        };
        if self.filter.is_empty() {
            items.iter().collect()
        } else {
            let filter_lower = self.filter.to_lowercase();
            items
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
        if self.search_filtered.is_empty() {
            self.search_table_state.select(None);
        } else {
            self.search_table_state.select(Some(0));
        }
    }

    /// Returns the list of items for the currently active selector.
    pub fn dropdown_items(&self) -> Vec<String> {
        match self.focus {
            Focus::Selector(SelectorTarget::Context) => self.contexts.clone(),
            Focus::Selector(SelectorTarget::Namespace) => self.namespaces.clone(),
            Focus::Selector(SelectorTarget::ResourceType) => {
                self.visible_resource_types()
                    .into_iter()
                    .map(|(label, _)| label)
                    .collect()
            }
            Focus::ResourceList => Vec::new(),
        }
    }

    /// Returns visible resource types as (display_label, ALL_index) pairs.
    pub fn visible_resource_types(&self) -> Vec<(String, usize)> {
        if self.resource_counts.is_empty() {
            ResourceType::ALL
                .iter()
                .enumerate()
                .map(|(i, t)| (t.to_string(), i))
                .collect()
        } else {
            ResourceType::ALL
                .iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    let count = self.resource_counts.get(t).copied().unwrap_or(0);
                    if count > 0 || self.selected_resource_types.contains(t) {
                        let label = if count > 0 {
                            format!("{} ({})", t, count)
                        } else {
                            t.to_string()
                        };
                        Some((label, i))
                    } else {
                        None
                    }
                })
                .collect()
        }
    }

    /// Maps a dropdown item index (for ResourceTypeSelector) back to a ResourceType::ALL index.
    fn resource_type_all_index(&self, dropdown_item_idx: usize) -> usize {
        let visible = self.visible_resource_types();
        visible
            .get(dropdown_item_idx)
            .map(|(_, all_idx)| *all_idx)
            .unwrap_or(0)
    }

    /// Open a selector overlay.
    pub fn open_selector(&mut self, target: SelectorTarget) {
        self.focus = Focus::Selector(target);
        self.dropdown_query.clear();
        self.dropdown_visible = true;
        self.dropdown_toggled.clear();

        // Pre-populate toggles with current selections
        match target {
            SelectorTarget::Context => {
                self.dropdown_toggled = self.selected_contexts.clone();
            }
            SelectorTarget::Namespace => {
                self.dropdown_toggled = self.selected_namespaces.clone();
            }
            SelectorTarget::ResourceType => {
                // Map selected types to visible indices
                let visible = self.visible_resource_types();
                for &rt in &self.selected_resource_types {
                    if let Some(pos) = visible.iter().position(|(_, all_idx)| {
                        ResourceType::ALL[*all_idx] == rt
                    }) {
                        self.dropdown_toggled.insert(pos);
                    }
                }
            }
        }

        self.update_dropdown_filter();
        // Pre-select the first item
        self.dropdown_selected = 0;
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
        if self.dropdown_filtered.is_empty() {
            self.dropdown_selected = 0;
        } else {
            self.dropdown_selected = self.dropdown_selected.min(self.dropdown_filtered.len() - 1);
        }
    }

    /// Confirm the dropdown selection (Enter). Selects all toggled items + the currently
    /// highlighted item, then closes the selector.
    fn dropdown_confirm(&mut self) -> InputAction {
        if !self.dropdown_visible {
            self.focus = Focus::ResourceList;
            return InputAction::None;
        }

        // Add the currently highlighted item to toggles (if not already)
        if let Some(&item_idx) = self.dropdown_filtered.get(self.dropdown_selected) {
            self.dropdown_toggled.insert(item_idx);
        }

        let action = match self.focus {
            Focus::Selector(SelectorTarget::Context) => {
                if self.dropdown_toggled.is_empty() {
                    InputAction::None
                } else if self.dropdown_toggled != self.selected_contexts {
                    self.selected_contexts = self.dropdown_toggled.clone();
                    InputAction::ContextChanged
                } else {
                    InputAction::None
                }
            }
            Focus::Selector(SelectorTarget::Namespace) => {
                if self.dropdown_toggled.is_empty() {
                    InputAction::None
                } else if self.dropdown_toggled != self.selected_namespaces {
                    self.selected_namespaces = self.dropdown_toggled.clone();
                    InputAction::NamespaceChanged
                } else {
                    InputAction::None
                }
            }
            Focus::Selector(SelectorTarget::ResourceType) => {
                let new_types: Vec<ResourceType> = self
                    .dropdown_toggled
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|idx| {
                        let all_idx = self.resource_type_all_index(idx);
                        ResourceType::ALL[all_idx]
                    })
                    .collect();

                if new_types.is_empty() {
                    InputAction::None
                } else {
                    // Sort by ALL index order for consistent display
                    let mut sorted: Vec<ResourceType> = new_types;
                    sorted.sort_by_key(|rt| {
                        ResourceType::ALL.iter().position(|t| t == rt).unwrap_or(0)
                    });
                    sorted.dedup();
                    if sorted != self.selected_resource_types {
                        self.selected_resource_types = sorted;
                        InputAction::ResourceTypeChanged
                    } else {
                        InputAction::None
                    }
                }
            }
            Focus::ResourceList => InputAction::None,
        };

        // Close selector and return to resource list
        self.focus = Focus::ResourceList;
        self.dropdown_visible = false;
        self.dropdown_toggled.clear();
        self.table_state.select(Some(0));
        action
    }

    pub fn handle_tick(&mut self) {
        if let Some(ref _msg) = self.error_message {
            self.error_ticks += 1;
            if self.error_ticks > 20 {
                self.error_message = None;
                self.error_ticks = 0;
            }
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg);
        self.error_ticks = 0;
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> InputAction {
        // Global quit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return InputAction::None;
        }

        // Global Ctrl+F to enter search (from List view only, not from selector or other modes)
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('f')
            && self.view_mode == ViewMode::List
            && self.focus == Focus::ResourceList
        {
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
                self.view_mode = ViewMode::List;
                InputAction::None
            }
        }
    }

    fn handle_list_input(&mut self, key: KeyEvent) -> InputAction {
        match self.focus {
            Focus::ResourceList => self.handle_resource_list_input(key),
            Focus::Selector(_) => self.handle_selector_input(key),
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
            // C/N/T to open selectors
            KeyCode::Char('c') => {
                self.open_selector(SelectorTarget::Context);
                InputAction::None
            }
            KeyCode::Char('n') => {
                self.open_selector(SelectorTarget::Namespace);
                InputAction::None
            }
            KeyCode::Char('t') => {
                self.open_selector(SelectorTarget::ResourceType);
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
                if let Some((_, rt)) = self.selected_resource() {
                    if rt.supports_logs() {
                        self.view_mode = ViewMode::Logs;
                        self.log_lines.clear();
                        self.log_scroll = 0;
                        self.log_follow = true;
                        return InputAction::StreamLogs;
                    }
                }
                InputAction::None
            }
            KeyCode::Char('d') => {
                if self.selected_resource().is_some() {
                    self.view_mode = ViewMode::Confirm(ConfirmAction::Delete);
                }
                InputAction::None
            }
            KeyCode::Char('r') => {
                if let Some((_, rt)) = self.selected_resource() {
                    if rt.supports_restart() {
                        self.view_mode = ViewMode::Confirm(ConfirmAction::Restart);
                    }
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
                InputAction::None
            }
            _ => InputAction::None,
        }
    }

    fn handle_selector_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Esc => {
                // Close selector, return to resource list (discard pending changes)
                self.focus = Focus::ResourceList;
                self.dropdown_visible = false;
                self.dropdown_toggled.clear();
                InputAction::None
            }
            KeyCode::Enter => {
                self.dropdown_confirm()
            }
            KeyCode::Char(' ') => {
                // Toggle selection of current item (multi-select)
                if let Some(&item_idx) = self.dropdown_filtered.get(self.dropdown_selected) {
                    if self.dropdown_toggled.contains(&item_idx) {
                        self.dropdown_toggled.remove(&item_idx);
                    } else {
                        self.dropdown_toggled.insert(item_idx);
                    }
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
                if !self.dropdown_query.is_empty() {
                    self.dropdown_query.pop();
                    self.dropdown_selected = 0;
                    self.update_dropdown_filter();
                }
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
        let rt = self.selected_row_resource_type();
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
                let lines = self.detail_text.lines().count() as u16;
                self.detail_scroll = lines.saturating_sub(10);
                InputAction::None
            }
            KeyCode::Char('g') => {
                self.detail_scroll = 0;
                InputAction::None
            }
            KeyCode::Char('l') => {
                if rt.map(|t| t.supports_logs()).unwrap_or(false)
                    && self.selected_resource().is_some()
                {
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
                if rt.map(|t| t.supports_restart()).unwrap_or(false)
                    && self.selected_resource().is_some()
                {
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
            KeyCode::Down => {
                self.search_select_next();
                InputAction::None
            }
            KeyCode::Up => {
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
        let rows = self.display_rows();
        let len = rows.len();
        if len == 0 {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0);
        // Move to next non-divider row
        let mut next = (current + 1) % len;
        let start = next;
        loop {
            if matches!(rows[next], DisplayRow::Resource { .. }) {
                break;
            }
            next = (next + 1) % len;
            if next == start {
                // All dividers, shouldn't happen
                break;
            }
        }
        self.table_state.select(Some(next));
    }

    fn select_prev(&mut self) {
        let rows = self.display_rows();
        let len = rows.len();
        if len == 0 {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0);
        let mut prev = if current == 0 { len - 1 } else { current - 1 };
        let start = prev;
        loop {
            if matches!(rows[prev], DisplayRow::Resource { .. }) {
                break;
            }
            prev = if prev == 0 { len - 1 } else { prev - 1 };
            if prev == start {
                break;
            }
        }
        self.table_state.select(Some(prev));
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
