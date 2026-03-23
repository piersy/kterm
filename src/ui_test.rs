#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;
    use crate::types::{ConfirmAction, Focus, ResourceItem, ResourceType, SelectorTarget, ViewMode};
    use crate::ui;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
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
        app.focus = Focus::ResourceList;
        app.contexts = vec!["gke-prod".to_string(), "minikube".to_string()];
        app.selected_contexts.clear();
        app.selected_contexts.insert(0);
        app.namespaces = vec!["default".to_string(), "kube-system".to_string()];
        app.selected_namespaces.clear();
        app.selected_namespaces.insert(0);
        app.resources_by_type.insert(
            ResourceType::Pods,
            vec![
                fake_pod("nginx-pod-0", "Running"),
                fake_pod("redis-pod-1", "Pending"),
                fake_pod("api-pod-2", "CrashLoopBackOff"),
            ],
        );
        app
    }

    fn render_to_string(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| ui::render(f, app)).unwrap();
        terminal.backend().to_string()
    }

    // --- List View Rendering ---

    #[test]
    fn test_list_view_renders_header_selectors() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("Cluster"),
            "Header should show Cluster selector"
        );
        assert!(
            output.contains("Namespace"),
            "Header should show Namespace selector"
        );
        assert!(output.contains("Type"), "Header should show Type selector");
    }

    #[test]
    fn test_list_view_renders_context_value() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("gke-prod"),
            "Header should show current context 'gke-prod', got:\n{}",
            output
        );
    }

    #[test]
    fn test_list_view_renders_namespace_value() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("default"),
            "Header should show current namespace 'default'"
        );
    }

    #[test]
    fn test_list_view_renders_resource_type() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("pods"),
            "Header should show resource type 'pods'"
        );
    }

    #[test]
    fn test_list_view_renders_column_headers() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(output.contains("NAME"), "Should show NAME column header");
        assert!(
            output.contains("STATUS"),
            "Should show STATUS column header"
        );
        assert!(output.contains("AGE"), "Should show AGE column header");
        assert!(
            output.contains("RESTARTS"),
            "Should show RESTARTS column header"
        );
        assert!(output.contains("NODE"), "Should show NODE column header");
    }

    #[test]
    fn test_list_view_renders_pod_names() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("nginx-pod-0"),
            "Should render pod name 'nginx-pod-0', got:\n{}",
            output
        );
        assert!(
            output.contains("redis-pod-1"),
            "Should render pod name 'redis-pod-1'"
        );
        assert!(
            output.contains("api-pod-2"),
            "Should render pod name 'api-pod-2'"
        );
    }

    #[test]
    fn test_list_view_renders_pod_statuses() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 140, 24);

        assert!(output.contains("Running"), "Should render Running status");
        assert!(output.contains("Pending"), "Should render Pending status");
        assert!(
            output.contains("CrashLoopBackOff"),
            "Should render CrashLoopBackOff status, got:\n{}",
            output
        );
    }

    #[test]
    fn test_list_view_renders_footer_keybindings() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(output.contains("q:Quit"), "Footer should show quit binding");
        assert!(
            output.contains("j/k:Nav"),
            "Footer should show navigation binding"
        );
        assert!(
            output.contains("Enter:Detail"),
            "Footer should show detail binding"
        );
    }

    #[test]
    fn test_list_view_renders_hotkey_indicators() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("[C]"),
            "Should show [C] hotkey for Cluster, got:\n{}",
            output
        );
        assert!(
            output.contains("[N]"),
            "Should show [N] hotkey for Namespace"
        );
        assert!(output.contains("[T]"), "Should show [T] hotkey for Type");
    }

    // --- PVC and StatefulSet Column Headers ---

    #[test]
    fn test_pvc_column_headers() {
        let mut app = App::new();
        app.focus = Focus::ResourceList;
        app.selected_resource_types = vec![ResourceType::PersistentVolumeClaims];
        app.resources_by_type.insert(
            ResourceType::PersistentVolumeClaims,
            vec![ResourceItem {
                name: "data-pvc".to_string(),
                namespace: "default".to_string(),
                status: "Bound".to_string(),
                age: "5d".to_string(),
                extra: vec![
                    ("volume".to_string(), "pv-abc".to_string()),
                    ("capacity".to_string(), "10Gi".to_string()),
                ],
                raw_yaml: String::new(),
            }],
        );
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("VOLUME"),
            "PVC view should show VOLUME column"
        );
        assert!(
            output.contains("CAPACITY"),
            "PVC view should show CAPACITY column"
        );
        assert!(
            output.contains("data-pvc"),
            "PVC view should show resource name"
        );
        assert!(output.contains("Bound"), "PVC view should show status");
    }

    #[test]
    fn test_statefulset_column_headers() {
        let mut app = App::new();
        app.focus = Focus::ResourceList;
        app.selected_resource_types = vec![ResourceType::StatefulSets];
        app.resources_by_type.insert(
            ResourceType::StatefulSets,
            vec![ResourceItem {
                name: "web-ss".to_string(),
                namespace: "default".to_string(),
                status: "Active".to_string(),
                age: "3d".to_string(),
                extra: vec![("ready".to_string(), "3/3".to_string())],
                raw_yaml: String::new(),
            }],
        );
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("READY"),
            "StatefulSet view should show READY column"
        );
        assert!(
            output.contains("web-ss"),
            "StatefulSet view should show resource name"
        );
        assert!(
            output.contains("3/3"),
            "StatefulSet view should show ready count"
        );
    }

    // --- Detail View Rendering ---

    #[test]
    fn test_detail_view_renders_split_pane() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Detail;
        app.detail_text =
            "Name:         nginx-pod-0\nNamespace:    default\nStatus:       Running\n".to_string();

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("nginx-pod-0"),
            "Detail view should show pod name in list and detail"
        );
        assert!(
            output.contains("Namespace"),
            "Detail view should show detail text"
        );
        assert!(
            output.contains("Esc:Back"),
            "Detail footer should show Esc:Back"
        );
    }

    // --- Logs View Rendering ---

    #[test]
    fn test_logs_view_renders_log_lines() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Logs;
        app.log_lines = vec![
            "2024-01-15 INFO Starting server".to_string(),
            "2024-01-15 WARN High memory usage".to_string(),
            "2024-01-15 ERROR Connection refused".to_string(),
        ];

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("Starting server"),
            "Logs view should show log lines"
        );
    }

    #[test]
    fn test_logs_view_shows_follow_indicator() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Logs;
        app.log_follow = true;
        app.log_lines = vec!["test log line".to_string()];

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("FOLLOW"),
            "Logs view should show FOLLOW indicator when follow is on"
        );
    }

    // --- Confirmation Dialog ---

    #[test]
    fn test_confirm_dialog_renders_overlay() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Confirm(ConfirmAction::Delete);

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("Confirm Delete"),
            "Confirm dialog should show action name, got:\n{}",
            output
        );
        assert!(
            output.contains("Are you sure"),
            "Confirm dialog should show confirmation prompt"
        );
    }

    // --- Filter Mode ---

    #[test]
    fn test_filter_mode_shows_filter_text_in_title() {
        let mut app = app_with_pods();
        app.filter = "nginx".to_string();

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("filter: nginx"),
            "Resource list title should show active filter, got:\n{}",
            output
        );
    }

    #[test]
    fn test_filter_mode_footer() {
        let mut app = app_with_pods();
        app.filter_active = true;

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("Esc:Cancel"),
            "Filter mode footer should show Esc:Cancel"
        );
        assert!(
            output.contains("Enter:Apply"),
            "Filter mode footer should show Enter:Apply"
        );
    }

    // --- Error Display ---

    #[test]
    fn test_error_message_renders_in_footer() {
        let mut app = app_with_pods();
        app.set_error("Connection timed out".to_string());

        let output = render_to_string(&mut app, 200, 24);

        assert!(
            output.contains("Connection timed out"),
            "Footer should show error message, got:\n{}",
            output
        );
    }

    // --- Empty State ---

    #[test]
    fn test_empty_resource_list_renders_without_panic() {
        let mut app = App::new();
        app.focus = Focus::ResourceList;
        let output = render_to_string(&mut app, 100, 24);
        assert!(
            output.contains("pods"),
            "Should still show resource type header"
        );
    }

    // --- Navigation Flow Integration ---

    #[test]
    fn test_navigate_then_render_shows_selection_change() {
        let mut app = app_with_pods();

        let output1 = render_to_string(&mut app, 100, 24);
        assert!(output1.contains("nginx-pod-0"));

        app.handle_input(key(KeyCode::Char('j')));
        let output2 = render_to_string(&mut app, 100, 24);
        assert!(output2.contains("nginx-pod-0"));
        assert!(output2.contains("redis-pod-1"));
    }

    #[test]
    fn test_enter_detail_then_render() {
        let mut app = app_with_pods();
        app.detail_text = "Name: nginx-pod-0\nStatus: Running".to_string();

        app.handle_input(key(KeyCode::Enter));
        assert_eq!(app.view_mode, ViewMode::Detail);

        let output = render_to_string(&mut app, 100, 24);
        assert!(
            output.contains("Esc:Back"),
            "After entering detail, footer should show back binding"
        );
    }

    // --- Small Terminal Size ---

    #[test]
    fn test_renders_at_minimum_size_without_panic() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 40, 16);
        assert!(!output.is_empty());
    }

    // --- Search View Rendering ---

    use crate::types::SearchResult;

    fn app_with_search() -> App {
        let mut app = App::new();
        app.view_mode = ViewMode::Search;
        app.contexts = vec!["gke-prod".to_string(), "gke-staging".to_string()];
        app.search_results = vec![
            SearchResult {
                resource: ResourceItem {
                    name: "op-geth-node-0".to_string(),
                    namespace: "ethereum".to_string(),
                    status: "Running".to_string(),
                    age: "1h".to_string(),
                    extra: vec![],
                    raw_yaml: String::new(),
                },
                context: "gke-prod".to_string(),
                resource_type: ResourceType::Pods,
            },
            SearchResult {
                resource: ResourceItem {
                    name: "op-geth-node-0".to_string(),
                    namespace: "ethereum".to_string(),
                    status: "Running".to_string(),
                    age: "2h".to_string(),
                    extra: vec![],
                    raw_yaml: String::new(),
                },
                context: "gke-staging".to_string(),
                resource_type: ResourceType::Pods,
            },
            SearchResult {
                resource: ResourceItem {
                    name: "redis-master-0".to_string(),
                    namespace: "cache".to_string(),
                    status: "Running".to_string(),
                    age: "3d".to_string(),
                    extra: vec![],
                    raw_yaml: String::new(),
                },
                context: "gke-prod".to_string(),
                resource_type: ResourceType::StatefulSets,
            },
        ];
        app.update_search_filter();
        app
    }

    #[test]
    fn test_search_view_renders_search_input() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("Search"),
            "Search view should show search input box, got:\n{}",
            output
        );
    }

    #[test]
    fn test_search_view_renders_column_headers() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("NAME"),
            "Search results should show NAME column"
        );
        assert!(
            output.contains("TYPE"),
            "Search results should show TYPE column"
        );
    }

    #[test]
    fn test_search_view_renders_resource_names() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("op-geth-node-0"),
            "Search results should show resource name, got:\n{}",
            output
        );
        assert!(
            output.contains("redis-master-0"),
            "Search results should show all matching resources"
        );
    }

    #[test]
    fn test_search_view_no_header_selectors() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        // Search view should NOT show the normal header selectors
        assert!(
            !output.contains("[C]"),
            "Search view should not show [C] selector hotkey"
        );
    }

    #[test]
    fn test_search_view_footer() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("Esc:Back"),
            "Search footer should show Esc:Back"
        );
        assert!(
            output.contains("Enter:Detail"),
            "Search footer should show Enter:Detail"
        );
    }

    #[test]
    fn test_search_empty_results_renders_without_panic() {
        let mut app = App::new();
        app.view_mode = ViewMode::Search;
        let output = render_to_string(&mut app, 100, 24);
        assert!(
            output.contains("Search"),
            "Should render search view with empty results"
        );
    }

    #[test]
    fn test_search_renders_at_minimum_size() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 40, 16);
        assert!(!output.is_empty());
    }

    // --- Selector Overlay Rendering ---

    #[test]
    fn test_selector_dropdown_renders_as_overlay() {
        let mut app = app_with_pods();
        app.open_selector(SelectorTarget::Context);

        let output = render_to_string(&mut app, 100, 24);

        // Should show the dropdown with context items
        assert!(
            output.contains("gke-prod"),
            "Dropdown should show context values, got:\n{}",
            output
        );
        assert!(
            output.contains("minikube"),
            "Dropdown should show all context values"
        );
    }

    #[test]
    fn test_selector_shows_toggle_indicators() {
        let mut app = app_with_pods();
        app.open_selector(SelectorTarget::ResourceType);

        let output = render_to_string(&mut app, 100, 24);

        // Should show [x] for toggled items and [ ] for untoggled
        assert!(
            output.contains("[x]") || output.contains("[ ]"),
            "Dropdown should show toggle indicators, got:\n{}",
            output
        );
    }
}
