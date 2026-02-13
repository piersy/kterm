#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use crate::app::{App, InputAction};
    use crate::types::{ConfirmAction, Focus, ResourceItem, ResourceType, ViewMode};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn key_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn fake_pod(name: &str, status: &str) -> ResourceItem {
        ResourceItem {
            name: name.to_string(),
            namespace: "default".to_string(),
            status: status.to_string(),
            age: "1h".to_string(),
            extra: vec![
                ("restarts".to_string(), "0".to_string()),
                ("node".to_string(), "node-a".to_string()),
            ],
            raw_yaml: "---\napiVersion: v1\nkind: Pod".to_string(),
        }
    }

    fn app_with_pods() -> App {
        let mut app = App::new();
        app.resources = vec![
            fake_pod("pod-0", "Running"),
            fake_pod("pod-1", "Pending"),
            fake_pod("pod-2", "Running"),
        ];
        app
    }

    #[test]
    fn test_quit_with_q() {
        let mut app = App::new();
        app.focus = Focus::ResourceList;
        app.handle_input(key(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_with_ctrl_c() {
        let mut app = App::new();
        app.handle_input(key_with_mod(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn test_navigate_down_j() {
        let mut app = app_with_pods();
        assert_eq!(app.table_state.selected(), Some(0));

        app.handle_input(key(KeyCode::Char('j')));
        assert_eq!(app.table_state.selected(), Some(1));

        app.handle_input(key(KeyCode::Char('j')));
        assert_eq!(app.table_state.selected(), Some(2));

        // Wrap around
        app.handle_input(key(KeyCode::Char('j')));
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_navigate_up_k() {
        let mut app = app_with_pods();
        assert_eq!(app.table_state.selected(), Some(0));

        // Wrap to end
        app.handle_input(key(KeyCode::Char('k')));
        assert_eq!(app.table_state.selected(), Some(2));

        app.handle_input(key(KeyCode::Char('k')));
        assert_eq!(app.table_state.selected(), Some(1));
    }

    #[test]
    fn test_navigate_with_arrow_keys() {
        let mut app = app_with_pods();
        app.handle_input(key(KeyCode::Down));
        assert_eq!(app.table_state.selected(), Some(1));

        app.handle_input(key(KeyCode::Up));
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_tab_cycles_focus() {
        let mut app = App::new();
        assert_eq!(app.focus, Focus::ResourceList);

        app.handle_input(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::ContextSelector);

        app.handle_input(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::NamespaceSelector);

        app.handle_input(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::ResourceTypeSelector);

        app.handle_input(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::ResourceList);
    }

    #[test]
    fn test_backtab_reverse_cycles_focus() {
        let mut app = App::new();
        assert_eq!(app.focus, Focus::ResourceList);

        app.handle_input(key(KeyCode::BackTab));
        assert_eq!(app.focus, Focus::ResourceTypeSelector);

        app.handle_input(key(KeyCode::BackTab));
        assert_eq!(app.focus, Focus::NamespaceSelector);
    }

    #[test]
    fn test_context_selector_h_l() {
        let mut app = App::new();
        app.contexts = vec![
            "ctx-1".to_string(),
            "ctx-2".to_string(),
            "ctx-3".to_string(),
        ];
        app.selected_context = 0;
        app.focus = Focus::ContextSelector;

        let action = app.handle_input(key(KeyCode::Char('l')));
        assert_eq!(action, InputAction::ContextChanged);
        assert_eq!(app.selected_context, 1);

        let action = app.handle_input(key(KeyCode::Char('l')));
        assert_eq!(action, InputAction::ContextChanged);
        assert_eq!(app.selected_context, 2);

        // Wrap
        let action = app.handle_input(key(KeyCode::Char('l')));
        assert_eq!(action, InputAction::ContextChanged);
        assert_eq!(app.selected_context, 0);

        // Go back with h
        let action = app.handle_input(key(KeyCode::Char('h')));
        assert_eq!(action, InputAction::ContextChanged);
        assert_eq!(app.selected_context, 2);
    }

    #[test]
    fn test_namespace_selector() {
        let mut app = App::new();
        app.namespaces = vec![
            "default".to_string(),
            "kube-system".to_string(),
        ];
        app.selected_namespace = 0;
        app.focus = Focus::NamespaceSelector;

        let action = app.handle_input(key(KeyCode::Right));
        assert_eq!(action, InputAction::NamespaceChanged);
        assert_eq!(app.selected_namespace, 1);

        let action = app.handle_input(key(KeyCode::Right));
        assert_eq!(action, InputAction::NamespaceChanged);
        assert_eq!(app.selected_namespace, 0);
    }

    #[test]
    fn test_resource_type_selector() {
        let mut app = App::new();
        app.focus = Focus::ResourceTypeSelector;
        assert_eq!(app.resource_type, ResourceType::Pods);

        let action = app.handle_input(key(KeyCode::Char('l')));
        assert_eq!(action, InputAction::ResourceTypeChanged);
        assert_eq!(app.resource_type, ResourceType::PersistentVolumeClaims);

        let action = app.handle_input(key(KeyCode::Char('l')));
        assert_eq!(action, InputAction::ResourceTypeChanged);
        assert_eq!(app.resource_type, ResourceType::StatefulSets);

        let action = app.handle_input(key(KeyCode::Char('h')));
        assert_eq!(action, InputAction::ResourceTypeChanged);
        assert_eq!(app.resource_type, ResourceType::PersistentVolumeClaims);
    }

    #[test]
    fn test_enter_detail_view() {
        let mut app = app_with_pods();
        let action = app.handle_input(key(KeyCode::Enter));
        assert_eq!(action, InputAction::Describe);
        assert_eq!(app.view_mode, ViewMode::Detail);
    }

    #[test]
    fn test_enter_on_empty_list_does_nothing() {
        let mut app = App::new();
        let action = app.handle_input(key(KeyCode::Enter));
        assert_eq!(action, InputAction::None);
        assert_eq!(app.view_mode, ViewMode::List);
    }

    #[test]
    fn test_esc_from_detail_returns_to_list() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Detail;

        let action = app.handle_input(key(KeyCode::Esc));
        assert_eq!(action, InputAction::None);
        assert_eq!(app.view_mode, ViewMode::List);
    }

    #[test]
    fn test_detail_scroll() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Detail;
        app.detail_text = "line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12\nline13\nline14\nline15".to_string();

        assert_eq!(app.detail_scroll, 0);
        app.handle_input(key(KeyCode::Char('j')));
        assert_eq!(app.detail_scroll, 1);

        app.handle_input(key(KeyCode::Char('k')));
        assert_eq!(app.detail_scroll, 0);

        // Can't scroll past 0
        app.handle_input(key(KeyCode::Char('k')));
        assert_eq!(app.detail_scroll, 0);

        // Jump to bottom
        app.handle_input(key(KeyCode::Char('G')));
        assert!(app.detail_scroll > 0);

        // Jump to top
        app.handle_input(key(KeyCode::Char('g')));
        assert_eq!(app.detail_scroll, 0);
    }

    #[test]
    fn test_logs_view_for_pods() {
        let mut app = app_with_pods();
        let action = app.handle_input(key(KeyCode::Char('l')));
        assert_eq!(action, InputAction::StreamLogs);
        assert_eq!(app.view_mode, ViewMode::Logs);
        assert!(app.log_follow);
    }

    #[test]
    fn test_logs_not_available_for_pvcs() {
        let mut app = app_with_pods();
        app.resource_type = ResourceType::PersistentVolumeClaims;
        let action = app.handle_input(key(KeyCode::Char('l')));
        assert_eq!(action, InputAction::None);
        assert_eq!(app.view_mode, ViewMode::List);
    }

    #[test]
    fn test_logs_follow_toggle() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Logs;
        assert!(app.log_follow);

        app.handle_input(key(KeyCode::Char('f')));
        assert!(!app.log_follow);

        app.handle_input(key(KeyCode::Char('f')));
        assert!(app.log_follow);
    }

    #[test]
    fn test_esc_from_logs_stops_stream() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Logs;

        let action = app.handle_input(key(KeyCode::Esc));
        assert_eq!(action, InputAction::StopLogs);
        assert_eq!(app.view_mode, ViewMode::List);
    }

    #[test]
    fn test_delete_confirm_flow() {
        let mut app = app_with_pods();

        // Press d -> should open confirm
        let action = app.handle_input(key(KeyCode::Char('d')));
        assert_eq!(action, InputAction::None);
        assert_eq!(app.view_mode, ViewMode::Confirm(ConfirmAction::Delete));

        // Press y -> confirm
        let action = app.handle_input(key(KeyCode::Char('y')));
        assert_eq!(action, InputAction::Delete);
        assert_eq!(app.view_mode, ViewMode::List);
    }

    #[test]
    fn test_delete_cancel_flow() {
        let mut app = app_with_pods();

        app.handle_input(key(KeyCode::Char('d')));
        assert_eq!(app.view_mode, ViewMode::Confirm(ConfirmAction::Delete));

        // Press n -> cancel
        let action = app.handle_input(key(KeyCode::Char('n')));
        assert_eq!(action, InputAction::None);
        assert_eq!(app.view_mode, ViewMode::List);
    }

    #[test]
    fn test_restart_confirm_flow() {
        let mut app = app_with_pods();

        app.handle_input(key(KeyCode::Char('r')));
        assert_eq!(app.view_mode, ViewMode::Confirm(ConfirmAction::Restart));

        let action = app.handle_input(key(KeyCode::Char('y')));
        assert_eq!(action, InputAction::Restart);
        assert_eq!(app.view_mode, ViewMode::List);
    }

    #[test]
    fn test_edit_action() {
        let mut app = app_with_pods();
        let action = app.handle_input(key(KeyCode::Char('e')));
        assert_eq!(action, InputAction::Edit);
    }

    #[test]
    fn test_filter_mode() {
        let mut app = app_with_pods();

        // Enter filter mode
        app.handle_input(key(KeyCode::Char('/')));
        assert!(app.filter_active);
        assert!(app.filter.is_empty());

        // Type filter text
        app.handle_input(key(KeyCode::Char('p')));
        app.handle_input(key(KeyCode::Char('o')));
        app.handle_input(key(KeyCode::Char('d')));
        assert_eq!(app.filter, "pod");

        // Backspace
        app.handle_input(key(KeyCode::Backspace));
        assert_eq!(app.filter, "po");

        // Apply filter with enter
        app.handle_input(key(KeyCode::Enter));
        assert!(!app.filter_active);
        assert_eq!(app.filter, "po");
    }

    #[test]
    fn test_filter_esc_cancels() {
        let mut app = app_with_pods();
        app.handle_input(key(KeyCode::Char('/')));
        app.handle_input(key(KeyCode::Char('x')));
        app.handle_input(key(KeyCode::Esc));
        assert!(!app.filter_active);
    }

    #[test]
    fn test_filtered_resources() {
        let mut app = app_with_pods();
        assert_eq!(app.filtered_resources().len(), 3);

        app.filter = "pod-0".to_string();
        assert_eq!(app.filtered_resources().len(), 1);
        assert_eq!(app.filtered_resources()[0].name, "pod-0");

        app.filter = "nonexistent".to_string();
        assert_eq!(app.filtered_resources().len(), 0);
    }

    #[test]
    fn test_error_auto_dismiss() {
        let mut app = App::new();
        app.set_error("test error".to_string());
        assert!(app.error_message.is_some());

        // Tick 20 times (should not dismiss yet)
        for _ in 0..20 {
            app.handle_tick();
        }
        assert!(app.error_message.is_some());

        // One more tick should dismiss
        app.handle_tick();
        assert!(app.error_message.is_none());
    }

    #[test]
    fn test_resource_type_cycling() {
        assert_eq!(ResourceType::Pods.next(), ResourceType::PersistentVolumeClaims);
        assert_eq!(ResourceType::PersistentVolumeClaims.next(), ResourceType::StatefulSets);
        assert_eq!(ResourceType::StatefulSets.next(), ResourceType::Pods);

        assert_eq!(ResourceType::Pods.prev(), ResourceType::StatefulSets);
        assert_eq!(ResourceType::StatefulSets.prev(), ResourceType::PersistentVolumeClaims);
    }

    #[test]
    fn test_focus_cycling() {
        assert_eq!(Focus::ResourceList.next(), Focus::ContextSelector);
        assert_eq!(Focus::ContextSelector.next(), Focus::NamespaceSelector);
        assert_eq!(Focus::NamespaceSelector.next(), Focus::ResourceTypeSelector);
        assert_eq!(Focus::ResourceTypeSelector.next(), Focus::ResourceList);

        assert_eq!(Focus::ResourceList.prev(), Focus::ResourceTypeSelector);
        assert_eq!(Focus::ContextSelector.prev(), Focus::ResourceList);
    }

    #[test]
    fn test_resource_item_columns_pods() {
        let item = fake_pod("my-pod", "Running");
        let cols = item.columns(ResourceType::Pods);
        assert_eq!(cols[0], "my-pod");
        assert_eq!(cols[1], "Running");
        assert_eq!(cols[2], "1h");
        assert_eq!(cols[3], "0");
        assert_eq!(cols[4], "node-a");
    }

    #[test]
    fn test_resource_item_columns_pvcs() {
        let item = ResourceItem {
            name: "my-pvc".to_string(),
            namespace: "default".to_string(),
            status: "Bound".to_string(),
            age: "2d".to_string(),
            extra: vec![
                ("volume".to_string(), "pv-001".to_string()),
                ("capacity".to_string(), "10Gi".to_string()),
            ],
            raw_yaml: String::new(),
        };
        let cols = item.columns(ResourceType::PersistentVolumeClaims);
        assert_eq!(cols[0], "my-pvc");
        assert_eq!(cols[1], "Bound");
        assert_eq!(cols[2], "pv-001");
        assert_eq!(cols[3], "10Gi");
        assert_eq!(cols[4], "2d");
    }

    #[test]
    fn test_resource_item_columns_statefulsets() {
        let item = ResourceItem {
            name: "my-ss".to_string(),
            namespace: "default".to_string(),
            status: "Active".to_string(),
            age: "5d".to_string(),
            extra: vec![("ready".to_string(), "3/3".to_string())],
            raw_yaml: String::new(),
        };
        let cols = item.columns(ResourceType::StatefulSets);
        assert_eq!(cols[0], "my-ss");
        assert_eq!(cols[1], "3/3");
        assert_eq!(cols[2], "5d");
    }

    #[test]
    fn test_detail_view_actions() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Detail;

        // Delete from detail
        app.handle_input(key(KeyCode::Char('d')));
        assert_eq!(app.view_mode, ViewMode::Confirm(ConfirmAction::Delete));
        app.view_mode = ViewMode::Detail;

        // Restart from detail
        app.handle_input(key(KeyCode::Char('r')));
        assert_eq!(app.view_mode, ViewMode::Confirm(ConfirmAction::Restart));
        app.view_mode = ViewMode::Detail;

        // Logs from detail
        let action = app.handle_input(key(KeyCode::Char('l')));
        assert_eq!(action, InputAction::StreamLogs);
        assert_eq!(app.view_mode, ViewMode::Logs);
    }

    #[test]
    fn test_navigate_empty_list() {
        let mut app = App::new();
        // Should not panic on empty list
        app.handle_input(key(KeyCode::Char('j')));
        app.handle_input(key(KeyCode::Char('k')));
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn test_q_from_detail_goes_back() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Detail;
        app.handle_input(key(KeyCode::Char('q')));
        assert_eq!(app.view_mode, ViewMode::List);
        assert!(!app.should_quit);
    }
}
