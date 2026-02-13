#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;
    use crate::types::{ConfirmAction, Focus, ResourceItem, ResourceType, ViewMode};
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
        app.contexts = vec!["gke-prod".to_string(), "minikube".to_string()];
        app.selected_context = 0;
        app.namespaces = vec!["default".to_string(), "kube-system".to_string()];
        app.selected_namespace = 0;
        app.resources = vec![
            fake_pod("nginx-pod-0", "Running"),
            fake_pod("redis-pod-1", "Pending"),
            fake_pod("api-pod-2", "CrashLoopBackOff"),
        ];
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

        assert!(output.contains("Context"), "Header should show Context selector");
        assert!(output.contains("Namespace"), "Header should show Namespace selector");
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
            output.contains("Pods"),
            "Header should show resource type 'Pods'"
        );
    }

    #[test]
    fn test_list_view_renders_column_headers() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        assert!(output.contains("NAME"), "Should show NAME column header");
        assert!(output.contains("STATUS"), "Should show STATUS column header");
        assert!(output.contains("AGE"), "Should show AGE column header");
        assert!(output.contains("RESTARTS"), "Should show RESTARTS column header");
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
        // Use wider terminal to avoid column truncation of long status strings
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
    fn test_list_view_renders_selector_arrows() {
        let mut app = app_with_pods();
        let output = render_to_string(&mut app, 100, 24);

        // Selectors use arrow indicators
        assert!(
            output.contains("◀") && output.contains("▶"),
            "Selectors should show left/right arrows"
        );
    }

    // --- PVC and StatefulSet Column Headers ---

    #[test]
    fn test_pvc_column_headers() {
        let mut app = App::new();
        app.resource_type = ResourceType::PersistentVolumeClaims;
        app.resources = vec![ResourceItem {
            name: "data-pvc".to_string(),
            namespace: "default".to_string(),
            status: "Bound".to_string(),
            age: "5d".to_string(),
            extra: vec![
                ("volume".to_string(), "pv-abc".to_string()),
                ("capacity".to_string(), "10Gi".to_string()),
            ],
            raw_yaml: String::new(),
        }];
        let output = render_to_string(&mut app, 100, 24);

        assert!(output.contains("VOLUME"), "PVC view should show VOLUME column");
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
        app.resource_type = ResourceType::StatefulSets;
        app.resources = vec![ResourceItem {
            name: "web-ss".to_string(),
            namespace: "default".to_string(),
            status: "Active".to_string(),
            age: "3d".to_string(),
            extra: vec![("ready".to_string(), "3/3".to_string())],
            raw_yaml: String::new(),
        }];
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("READY"),
            "StatefulSet view should show READY column"
        );
        assert!(
            output.contains("web-ss"),
            "StatefulSet view should show resource name"
        );
        assert!(output.contains("3/3"), "StatefulSet view should show ready count");
    }

    // --- Switching Resource Type Updates Columns ---

    #[test]
    fn test_switching_resource_type_changes_columns() {
        let mut app = app_with_pods();
        let pods_output = render_to_string(&mut app, 100, 24);
        assert!(pods_output.contains("RESTARTS"));
        assert!(!pods_output.contains("VOLUME"));

        // Switch to PVCs
        app.resource_type = ResourceType::PersistentVolumeClaims;
        app.resources = vec![ResourceItem {
            name: "my-pvc".to_string(),
            namespace: "default".to_string(),
            status: "Bound".to_string(),
            age: "1d".to_string(),
            extra: vec![
                ("volume".to_string(), "pv-001".to_string()),
                ("capacity".to_string(), "5Gi".to_string()),
            ],
            raw_yaml: String::new(),
        }];
        let pvc_output = render_to_string(&mut app, 100, 24);
        assert!(pvc_output.contains("VOLUME"));
        assert!(!pvc_output.contains("RESTARTS"));
    }

    // --- Detail View Rendering ---

    #[test]
    fn test_detail_view_renders_split_pane() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Detail;
        app.detail_text = "Name:         nginx-pod-0\nNamespace:    default\nStatus:       Running\n".to_string();

        let output = render_to_string(&mut app, 100, 24);

        // Should still show the resource list on the left
        assert!(
            output.contains("nginx-pod-0"),
            "Detail view should show pod name in list and detail"
        );
        // Should show detail content
        assert!(
            output.contains("Namespace"),
            "Detail view should show detail text"
        );
        // Footer should show detail keybindings
        assert!(
            output.contains("Esc:Back"),
            "Detail footer should show Esc:Back"
        );
    }

    #[test]
    fn test_detail_view_shows_detail_keybindings() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Detail;
        app.detail_text = "Some detail text".to_string();

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("j/k:Scroll"),
            "Detail footer should show scroll bindings"
        );
        assert!(
            output.contains("g/G:Top/Bottom"),
            "Detail footer should show jump bindings"
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
        assert!(
            output.contains("High memory usage"),
            "Logs view should show warning lines"
        );
        assert!(
            output.contains("Connection refused"),
            "Logs view should show error lines"
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

    #[test]
    fn test_logs_view_shows_line_count() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Logs;
        app.log_lines = vec![
            "line 1".to_string(),
            "line 2".to_string(),
            "line 3".to_string(),
        ];

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("3 lines"),
            "Logs title should show line count, got:\n{}",
            output
        );
    }

    #[test]
    fn test_logs_view_shows_log_keybindings() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Logs;

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("f:Follow"),
            "Log footer should show follow toggle binding"
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

    #[test]
    fn test_confirm_restart_dialog() {
        let mut app = app_with_pods();
        app.view_mode = ViewMode::Confirm(ConfirmAction::Restart);

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("Confirm Restart"),
            "Confirm dialog should show 'Confirm Restart'"
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

        // Use wider terminal so the error message isn't clipped by the footer
        let output = render_to_string(&mut app, 140, 24);

        assert!(
            output.contains("Connection timed out"),
            "Footer should show error message, got:\n{}",
            output
        );
    }

    // --- Focus Indicator ---

    #[test]
    fn test_focused_selector_uses_arrows() {
        let mut app = app_with_pods();
        app.focus = Focus::ContextSelector;

        // Render with context focused
        let output_ctx = render_to_string(&mut app, 100, 24);
        assert!(
            output_ctx.contains("gke-prod"),
            "Context selector should show value when focused"
        );

        // Switch focus to namespace
        app.focus = Focus::NamespaceSelector;
        let output_ns = render_to_string(&mut app, 100, 24);
        assert!(
            output_ns.contains("default"),
            "Namespace selector should show value when focused"
        );
    }

    // --- Empty State ---

    #[test]
    fn test_empty_resource_list_renders_without_panic() {
        let mut app = App::new();
        app.resources = vec![];
        // Should not panic
        let output = render_to_string(&mut app, 100, 24);
        assert!(output.contains("Pods"), "Should still show resource type header");
    }

    // --- Navigation Flow Integration ---

    #[test]
    fn test_navigate_then_render_shows_selection_change() {
        let mut app = app_with_pods();

        // Initial render - first pod selected
        let output1 = render_to_string(&mut app, 100, 24);
        assert!(output1.contains("nginx-pod-0"));

        // Navigate down
        app.handle_input(key(KeyCode::Char('j')));
        let output2 = render_to_string(&mut app, 100, 24);
        // Both renders should show all pods
        assert!(output2.contains("nginx-pod-0"));
        assert!(output2.contains("redis-pod-1"));
    }

    #[test]
    fn test_enter_detail_then_render() {
        let mut app = app_with_pods();
        app.detail_text = "Name: nginx-pod-0\nStatus: Running".to_string();

        // Enter detail view
        app.handle_input(key(KeyCode::Enter));
        assert_eq!(app.view_mode, ViewMode::Detail);

        let output = render_to_string(&mut app, 100, 24);
        assert!(
            output.contains("Esc:Back"),
            "After entering detail, footer should show back binding"
        );
    }

    #[test]
    fn test_full_flow_list_to_detail_to_list() {
        let mut app = app_with_pods();

        // Start in list view
        let list_output = render_to_string(&mut app, 100, 24);
        assert!(list_output.contains("Enter:Detail"));

        // Enter detail
        app.handle_input(key(KeyCode::Enter));
        app.detail_text = "Detail content here".to_string();
        let detail_output = render_to_string(&mut app, 100, 24);
        assert!(detail_output.contains("Esc:Back"));
        assert!(detail_output.contains("Detail content here"));

        // Go back to list
        app.handle_input(key(KeyCode::Esc));
        let back_output = render_to_string(&mut app, 100, 24);
        assert!(back_output.contains("Enter:Detail"));
    }

    #[test]
    fn test_full_flow_list_to_confirm_delete_cancel() {
        let mut app = app_with_pods();

        // Open delete confirm
        app.handle_input(key(KeyCode::Char('d')));
        let confirm_output = render_to_string(&mut app, 100, 24);
        assert!(confirm_output.contains("Confirm Delete"));
        assert!(confirm_output.contains("y:Confirm"));

        // Cancel
        app.handle_input(key(KeyCode::Char('n')));
        let list_output = render_to_string(&mut app, 100, 24);
        assert!(list_output.contains("Enter:Detail"));
        assert!(!list_output.contains("Confirm Delete"));
    }

    #[test]
    fn test_full_flow_type_switch_rerenders_columns() {
        let mut app = app_with_pods();
        app.focus = Focus::ResourceTypeSelector;

        // Start with Pods
        let pods_output = render_to_string(&mut app, 100, 24);
        assert!(pods_output.contains("NODE"));

        // Switch to StatefulSets
        app.handle_input(key(KeyCode::Char('l'))); // Pods -> PVCs
        app.handle_input(key(KeyCode::Char('l'))); // PVCs -> StatefulSets
        app.resources = vec![ResourceItem {
            name: "web".to_string(),
            namespace: "default".to_string(),
            status: "Active".to_string(),
            age: "2d".to_string(),
            extra: vec![("ready".to_string(), "2/2".to_string())],
            raw_yaml: String::new(),
        }];
        let ss_output = render_to_string(&mut app, 100, 24);
        assert!(ss_output.contains("READY"));
        assert!(ss_output.contains("StatefulSets"));
        assert!(!ss_output.contains("NODE"));
    }

    // --- Small Terminal Size ---

    #[test]
    fn test_renders_at_minimum_size_without_panic() {
        let mut app = app_with_pods();
        // Small but not absurdly small - enough for the layout constraints
        let output = render_to_string(&mut app, 40, 16);
        // Should not panic, and should contain at least some content
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
        assert!(
            output.contains("NAMESPACE"),
            "Search results should show NAMESPACE column"
        );
        assert!(
            output.contains("CLUSTER"),
            "Search results should show CLUSTER column"
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
    fn test_search_view_renders_namespaces() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("ethereum"),
            "Search results should show namespace"
        );
        assert!(
            output.contains("cache"),
            "Search results should show namespace for all results"
        );
    }

    #[test]
    fn test_search_view_renders_cluster_names() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("gke-prod"),
            "Search results should show cluster name"
        );
        assert!(
            output.contains("gke-staging"),
            "Search results should show all cluster names"
        );
    }

    #[test]
    fn test_search_view_renders_resource_types() {
        let mut app = app_with_search();
        // Use wider terminal to ensure all columns render
        let output = render_to_string(&mut app, 120, 24);

        assert!(
            output.contains("Pods"),
            "Search results should show resource type, got:\n{}",
            output
        );
        assert!(
            output.contains("StatefulSets"),
            "Search results should show StatefulSets type"
        );
    }

    #[test]
    fn test_search_view_shows_result_count() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("3 found"),
            "Search results should show count, got:\n{}",
            output
        );
    }

    #[test]
    fn test_search_view_shows_scanning_indicator() {
        let mut app = app_with_search();
        app.search_loading = true;
        app.search_contexts_total = 3;
        app.search_contexts_done = 1;

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("scanning"),
            "Should show scanning indicator when loading, got:\n{}",
            output
        );
    }

    #[test]
    fn test_search_view_shows_search_query() {
        let mut app = app_with_search();
        app.search_query = "op-geth".to_string();
        app.update_search_filter();

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("op-geth"),
            "Search input should show current query, got:\n{}",
            output
        );
    }

    #[test]
    fn test_search_view_no_header_selectors() {
        let mut app = app_with_search();
        let output = render_to_string(&mut app, 100, 24);

        // Search view should NOT show the normal header selectors
        assert!(
            !output.contains("Context"),
            "Search view should not show Context selector"
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
    fn test_search_detail_view_full_screen() {
        let mut app = app_with_search();
        app.view_mode = ViewMode::Detail;
        app.entered_from_search = true;
        app.detail_text = "Name: op-geth-node-0\nNamespace: ethereum\nStatus: Running".to_string();

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("op-geth-node-0"),
            "Search detail should show resource name"
        );
        assert!(
            output.contains("Back to search"),
            "Search detail footer should mention back to search, got:\n{}",
            output
        );
        // Should NOT show header selectors
        assert!(
            !output.contains("Context"),
            "Search detail should not show Context selector"
        );
    }

    #[test]
    fn test_search_logs_view_full_screen() {
        let mut app = app_with_search();
        app.view_mode = ViewMode::Logs;
        app.entered_from_search = true;
        app.log_lines = vec!["INFO Starting".to_string()];

        let output = render_to_string(&mut app, 100, 24);

        assert!(
            output.contains("Starting"),
            "Search logs should show log lines"
        );
        assert!(
            output.contains("Back to search"),
            "Search logs footer should mention back to search, got:\n{}",
            output
        );
    }

    #[test]
    fn test_search_empty_results_renders_without_panic() {
        let mut app = App::new();
        app.view_mode = ViewMode::Search;
        // Should not panic with empty results
        let output = render_to_string(&mut app, 100, 24);
        assert!(
            output.contains("Search"),
            "Should render search view with empty results"
        );
    }

    #[test]
    fn test_search_renders_at_minimum_size() {
        let mut app = app_with_search();
        // Should not panic at minimum size
        let output = render_to_string(&mut app, 40, 16);
        assert!(!output.is_empty());
    }
}
