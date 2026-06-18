use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;

use crate::types::{
    fuzzy_match, ConfirmAction, Focus, ResourceItem, ResourceType, SearchResult, SelectorTarget,
    ViewMode, ALL_NAMESPACES_LABEL,
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

    // Related-components view
    /// Label key (from config) that groups related components.
    pub related_label: String,
    /// The label value being shown (for the related view's title).
    pub related_label_value: String,
    /// Namespace the related components were fetched from.
    pub related_namespace: String,
    /// Separately-fetched related resources (kept out of the live watched
    /// `resources_by_type` so the watch is undisturbed).
    pub related_by_type: HashMap<ResourceType, Vec<ResourceItem>>,
    /// Ordered list of types that have at least one related resource.
    pub related_types: Vec<ResourceType>,
    /// True while the related-components fetch is in flight.
    pub related_loading: bool,
    /// Monotonic id of the current related-components request. Bumped on each
    /// open so a late result from a superseded request (e.g. a different
    /// namespace with a colliding label value) is discarded.
    pub related_request: u64,
    /// View to restore when leaving the related view with Esc.
    pub previous_view: ViewMode,

    // Filter
    pub filter: String,
    pub filter_active: bool,

    // Error popup (modal, dismissed with any key)
    pub error_message: Option<String>,
    pub error_popup: bool,

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

    // Clusters that failed connectivity probes at startup (or on switch).
    // These are excluded from search and shown as "(unreachable)" in the UI.
    // Cleared for a context when the user explicitly selects it and it connects.
    pub unreachable_contexts: HashSet<String>,

    // Number of cluster probes still in flight at startup.
    pub cluster_probes_pending: usize,

    // Generation counter: incremented on context/namespace/type changes.
    // Used to discard stale watcher events from previous generations.
    pub generation: u64,

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
            namespaces: vec![
                ALL_NAMESPACES_LABEL.to_string(),
                "default".to_string(),
            ],
            selected_namespaces: {
                let mut s = HashSet::new();
                // Start scoped to "default" (index 1), not the all-namespaces
                // sentinel at index 0.
                s.insert(1);
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

            related_label: crate::config::DEFAULT_RELATED_LABEL.to_string(),
            related_label_value: String::new(),
            related_namespace: String::new(),
            related_by_type: HashMap::new(),
            related_types: Vec::new(),
            related_loading: false,
            related_request: 0,
            previous_view: ViewMode::List,

            filter: String::new(),
            filter_active: false,

            error_message: None,
            error_popup: false,

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
            unreachable_contexts: HashSet::new(),
            cluster_probes_pending: 0,

            generation: 0,

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

    /// Build the flat list of display rows for the multi-type view.
    ///
    /// View-aware: in [`ViewMode::Related`] it flattens the separately-fetched
    /// related resources (always with type dividers, no name filter); otherwise
    /// it flattens the live watched resources for the selected types.
    pub fn display_rows(&self) -> Vec<DisplayRow> {
        if self.view_mode == ViewMode::Related {
            return self.related_display_rows();
        }

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

    /// Flatten the related-components dataset into display rows, always grouped
    /// under a per-type divider so the originating types are labelled.
    fn related_display_rows(&self) -> Vec<DisplayRow> {
        let mut rows = Vec::new();
        for &rt in &self.related_types {
            rows.push(DisplayRow::TypeDivider(rt));
            if let Some(items) = self.related_by_type.get(&rt) {
                for i in 0..items.len() {
                    rows.push(DisplayRow::Resource {
                        resource_type: rt,
                        index: i,
                    });
                }
            }
        }
        rows
    }

    /// The resource map backing the current view (related vs live).
    fn current_resources(&self) -> &HashMap<ResourceType, Vec<ResourceItem>> {
        if self.view_mode == ViewMode::Related {
            &self.related_by_type
        } else {
            &self.resources_by_type
        }
    }

    /// Look up the [`ResourceItem`] for a [`DisplayRow::Resource`] cell in the
    /// current view's dataset. Used by the renderer.
    pub fn row_item(&self, resource_type: ResourceType, index: usize) -> Option<&ResourceItem> {
        self.current_resources().get(&resource_type)?.get(index)
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
                let item = self.current_resources().get(resource_type)?.get(*index)?;
                Some((item, *resource_type))
            }
            DisplayRow::TypeDivider(_) => None,
        }
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
            scored.sort_by_key(|s| Reverse(s.1));
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
            Focus::Selector(SelectorTarget::Context) => self
                .contexts
                .iter()
                .map(|c| {
                    if self.unreachable_contexts.contains(c) {
                        format!("{} (unreachable)", c)
                    } else {
                        c.clone()
                    }
                })
                .collect(),
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
                    // Distinguish between:
                    //   Some(n) where n > 0 : type has resources -> show with count
                    //   Some(0)             : type verified empty -> hide unless selected
                    //   None                : count fetch failed (timeout/error) -> show
                    //                         (don't hide types just because their count
                    //                         request failed)
                    match self.resource_counts.get(t) {
                        Some(&count) if count > 0 => {
                            Some((format!("{} ({})", t, count), i))
                        }
                        Some(_) => {
                            // Verified zero — only show if currently selected
                            if self.selected_resource_types.contains(t) {
                                Some((t.to_string(), i))
                            } else {
                                None
                            }
                        }
                        None => {
                            // Count unknown (fetch failed/timed out) — show the
                            // type so the user can still select it
                            Some((t.to_string(), i))
                        }
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
            scored.sort_by_key(|s| Reverse(s.1));
            self.dropdown_filtered = scored.into_iter().map(|(i, _)| i).collect();
        }

        // Pin the "all namespaces" entry to the top of the namespace selector,
        // regardless of fuzzy-match score. It is always shown so the user can
        // select cluster-wide scoping at any time.
        if matches!(self.focus, Focus::Selector(SelectorTarget::Namespace)) {
            if let Some(all_idx) = items
                .iter()
                .position(|it| it == ALL_NAMESPACES_LABEL)
            {
                self.dropdown_filtered.retain(|&i| i != all_idx);
                self.dropdown_filtered.insert(0, all_idx);
            }
        }

        if self.dropdown_filtered.is_empty() {
            self.dropdown_selected = 0;
        } else {
            self.dropdown_selected =
                self.dropdown_selected.min(self.dropdown_filtered.len().saturating_sub(1));
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
        self.select_first_row();
        action
    }

    pub fn handle_tick(&mut self) {
        // Error popup is modal — no auto-dismiss, user must press a key.
    }

    pub fn set_error(&mut self, msg: String) {
        crate::logging::log_error(&msg);
        self.error_message = Some(msg);
        self.error_popup = true;
    }

    pub fn dismiss_error_popup(&mut self) {
        self.error_popup = false;
        self.error_message = None;
    }

    /// Increment the generation counter, invalidating all in-flight events.
    pub fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    pub fn handle_input(&mut self, key: KeyEvent) -> InputAction {
        // Global quit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return InputAction::None;
        }

        // Dismiss error popup on any key
        if self.error_popup {
            self.dismiss_error_popup();
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
            ViewMode::Related => self.handle_related_input(key),
            ViewMode::Search => self.handle_search_input(key),
        }
    }

    /// Handle input while the related-components view is open: navigate the
    /// list, or Esc to return to the previous view.
    fn handle_related_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.leave_related_view();
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
            _ => InputAction::None,
        }
    }

    /// Restore the view that was active before the related-components view and
    /// clear the related dataset.
    fn leave_related_view(&mut self) {
        self.view_mode = self.previous_view;
        self.related_by_type.clear();
        self.related_types.clear();
        self.related_label_value.clear();
        self.related_loading = false;
        // Reset selection so a stale related-row index isn't reused by the
        // restored view.
        self.select_first_row();
    }

    /// Open the related-components view for the currently selected resource.
    ///
    /// Reads the configured label's value from the selection, captures its
    /// namespace, and triggers the async fetch. If the resource carries no such
    /// label there is nothing related to show, so an error popup is shown and
    /// the view does not change.
    fn open_related_view(&mut self) -> InputAction {
        // Resolve the label value + namespace before mutating self (avoids
        // overlapping the immutable borrow from `selected_resource`).
        let info = self.selected_resource().and_then(|(res, _)| {
            res.label(&self.related_label).map(|value| {
                let ns = if res.namespace.is_empty() {
                    String::new()
                } else {
                    res.namespace.clone()
                };
                (value, ns)
            })
        });
        match info {
            Some((value, ns)) => {
                let ns = if ns.is_empty() {
                    self.current_namespace().to_string()
                } else {
                    ns
                };
                self.previous_view = self.view_mode;
                self.related_label_value = value;
                self.related_namespace = ns;
                self.related_by_type.clear();
                self.related_types.clear();
                self.related_loading = true;
                // New request: supersedes any in-flight fetch.
                self.related_request = self.related_request.wrapping_add(1);
                self.view_mode = ViewMode::Related;
                self.table_state.select(None);
                InputAction::RelatedComponents
            }
            None => {
                self.error_message = Some(format!(
                    "Selected resource has no '{}' label — no related components.",
                    self.related_label
                ));
                self.error_popup = true;
                InputAction::None
            }
        }
    }

    /// Populate the related-components dataset from a completed fetch and select
    /// the first row.
    pub fn set_related_resources(&mut self, results: Vec<(ResourceType, Vec<ResourceItem>)>) {
        self.related_types = results.iter().map(|(rt, _)| *rt).collect();
        self.related_by_type = results.into_iter().collect();
        self.related_loading = false;
        self.select_first_row();
    }

    /// Apply a completed related-components fetch only if it still matches the
    /// current request: the related view is open, a fetch is in flight, and the
    /// request id matches. A superseded result (the user left the view or
    /// re-triggered for a different resource/namespace) is discarded. Returns
    /// whether the result was applied.
    pub fn apply_related_resources(
        &mut self,
        request: u64,
        results: Vec<(ResourceType, Vec<ResourceItem>)>,
    ) -> bool {
        if self.view_mode == ViewMode::Related
            && self.related_loading
            && self.related_request == request
        {
            self.set_related_resources(results);
            true
        } else {
            false
        }
    }

    fn handle_filter_input(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Esc => {
                self.filter_active = false;
            }
            KeyCode::Enter | KeyCode::Up | KeyCode::Down => {
                self.filter_active = false;
                self.select_first_row();
                if key.code == KeyCode::Down {
                    // Already on first row from select_first_row
                } else if key.code == KeyCode::Up {
                    // Select last row
                    let rows = self.display_rows();
                    if !rows.is_empty() {
                        for i in (0..rows.len()).rev() {
                            if matches!(rows[i], DisplayRow::Resource { .. }) {
                                self.table_state.select(Some(i));
                                break;
                            }
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.select_first_row();
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.select_first_row();
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
            // Related components (objects sharing the configured label value).
            KeyCode::Char('r') => self.open_related_view(),
            // Restart is now 'R' (lower-case 'r' opens related components).
            KeyCode::Char('R') => {
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
            KeyCode::Char('x') => {
                if let Some((_, rt)) = self.selected_resource() {
                    if rt.supports_exec() {
                        return InputAction::Exec;
                    }
                }
                InputAction::None
            }
            KeyCode::Char('/') => {
                self.filter_active = true;
                // Keep existing filter text so user can continue editing
                InputAction::None
            }
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.select_first_row();
                }
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
            // Restart is now 'R' in the detail view too (see list view).
            KeyCode::Char('R') => {
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
            KeyCode::Char('x') => {
                if rt.map(|t| t.supports_exec()).unwrap_or(false)
                    && self.selected_resource().is_some()
                {
                    InputAction::Exec
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

    /// Select the first non-divider row in the display, or None if empty.
    pub fn select_first_row(&mut self) {
        let rows = self.display_rows();
        if rows.is_empty() {
            self.table_state.select(None);
            return;
        }
        for (i, row) in rows.iter().enumerate() {
            if matches!(row, DisplayRow::Resource { .. }) {
                self.table_state.select(Some(i));
                return;
            }
        }
        self.table_state.select(None);
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
    RelatedComponents,
    Edit,
    Exec,
    OpenLogsInEditor,
    OpenLogsInLess,
    StartSearch,
    SearchDescribe,
    SearchStreamLogs,
}
