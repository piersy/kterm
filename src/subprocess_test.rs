#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use tokio::sync::mpsc;

    use crate::app::{App, InputAction};
    use crate::event::{AppEvent, EventHandler};
    use crate::types::{ResourceItem, ResourceType, ViewMode};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn fake_pod(name: &str) -> ResourceItem {
        ResourceItem {
            name: name.to_string(),
            namespace: "default".to_string(),
            status: "Running".to_string(),
            age: "1h".to_string(),
            extra: vec![
                ("restarts".to_string(), "0".to_string()),
                ("node".to_string(), "node-a".to_string()),
            ],
            raw_yaml: "---\napiVersion: v1\nkind: Pod".to_string(),
        }
    }

    fn app_in_logs_view() -> App {
        let mut app = App::new();
        app.resources = vec![fake_pod("pod-0")];
        app.resource_type = ResourceType::Pods;
        app.view_mode = ViewMode::Logs;
        app.log_lines = vec![
            "log line 1".to_string(),
            "log line 2".to_string(),
            "log line 3".to_string(),
        ];
        app.log_follow = true;
        app
    }

    // -----------------------------------------------------------------------
    // EventHandler suspend/resume API tests
    // -----------------------------------------------------------------------

    /// Verify that EventHandler exposes suspend() and resume() methods
    /// that can be called to stop stdin reading during subprocess execution.
    #[tokio::test]
    async fn test_event_handler_has_suspend_resume() {
        let mut handler = EventHandler::new();

        // suspend() should be callable without panic
        handler.suspend();

        // resume() should be callable after suspend without panic
        handler.resume();

        // After resume, the handler should still be functional —
        // we can send events and receive them.
        let tx = handler.sender();
        tx.send(AppEvent::Tick).unwrap();
        let event = handler.next().await;
        assert!(event.is_some());
    }

    /// Verify that suspend() drains stale Key events from the channel
    /// while preserving non-input events (K8s events, ticks, etc).
    #[tokio::test]
    async fn test_suspend_drains_stale_key_events() {
        let mut handler = EventHandler::new();
        let tx = handler.sender();

        // Simulate stale events in the channel: mix of Key events and K8s events
        tx.send(AppEvent::Key(key(KeyCode::Char('a')))).unwrap();
        tx.send(AppEvent::K8sError("some error".to_string()))
            .unwrap();
        tx.send(AppEvent::Key(key(KeyCode::Char('b')))).unwrap();
        tx.send(AppEvent::LogLine("log".to_string())).unwrap();
        tx.send(AppEvent::Key(key(KeyCode::Char('c')))).unwrap();

        // Suspend should discard Key events but keep K8s events
        handler.suspend();

        // The remaining events should be only the non-Key ones
        let mut remaining = Vec::new();
        while let Ok(event) = handler.try_recv() {
            remaining.push(event);
        }

        assert_eq!(remaining.len(), 2);
        assert!(matches!(remaining[0], AppEvent::K8sError(_)));
        assert!(matches!(remaining[1], AppEvent::LogLine(_)));
    }

    /// Verify that resume() also drains stale key events that may have
    /// arrived between suspend() and resume() (e.g. during terminal restore).
    #[tokio::test]
    async fn test_resume_drains_stale_key_events() {
        let mut handler = EventHandler::new();
        let tx = handler.sender();

        handler.suspend();

        // Events arriving after suspend but before resume
        tx.send(AppEvent::Key(key(KeyCode::Char('x')))).unwrap();
        tx.send(AppEvent::DetailLoaded("detail".to_string()))
            .unwrap();
        tx.send(AppEvent::Key(key(KeyCode::Char('y')))).unwrap();

        // Resume should drain stale key events
        handler.resume();

        // Only the DetailLoaded event should remain
        // (plus any Tick events from the tick task, so check non-Tick non-Key)
        let mut found_detail = false;
        let mut found_stale_key = false;
        while let Ok(event) = handler.try_recv() {
            match event {
                AppEvent::DetailLoaded(_) => found_detail = true,
                AppEvent::Key(_) => found_stale_key = true,
                _ => {} // Tick events are fine
            }
        }

        assert!(found_detail, "DetailLoaded should be preserved");
        assert!(!found_stale_key, "Stale Key events should be drained");
    }

    /// Verify suspend drains Resize events too (not just Key events).
    #[tokio::test]
    async fn test_suspend_drains_resize_events() {
        let mut handler = EventHandler::new();
        let tx = handler.sender();

        tx.send(AppEvent::Resize(80, 24)).unwrap();
        tx.send(AppEvent::LogLine("kept".to_string())).unwrap();
        tx.send(AppEvent::Resize(120, 40)).unwrap();

        handler.suspend();

        let mut remaining = Vec::new();
        while let Ok(event) = handler.try_recv() {
            remaining.push(event);
        }

        assert_eq!(remaining.len(), 1);
        assert!(matches!(remaining[0], AppEvent::LogLine(_)));
    }

    // -----------------------------------------------------------------------
    // Verify subprocess launch actions still route correctly
    // -----------------------------------------------------------------------

    /// Pressing 'o' in Logs view returns OpenLogsInEditor.
    #[test]
    fn test_open_in_editor_action() {
        let mut app = app_in_logs_view();
        let action = app.handle_input(key(KeyCode::Char('o')));
        assert_eq!(action, InputAction::OpenLogsInEditor);
    }

    /// Pressing 'O' in Logs view returns OpenLogsInLess.
    #[test]
    fn test_open_in_less_action() {
        let mut app = app_in_logs_view();
        let action = app.handle_input(key(KeyCode::Char('O')));
        assert_eq!(action, InputAction::OpenLogsInLess);
    }

    /// 'e' from resource list returns Edit.
    #[test]
    fn test_edit_yaml_action() {
        let mut app = App::new();
        app.focus = crate::types::Focus::ResourceList;
        app.resources = vec![fake_pod("pod-0")];
        app.resource_type = ResourceType::Pods;
        let action = app.handle_input(key(KeyCode::Char('e')));
        assert_eq!(action, InputAction::Edit);
    }

    /// 'e' from detail view returns Edit.
    #[test]
    fn test_edit_from_detail_view() {
        let mut app = App::new();
        app.resources = vec![fake_pod("pod-0")];
        app.resource_type = ResourceType::Pods;
        app.view_mode = ViewMode::Detail;
        let action = app.handle_input(key(KeyCode::Char('e')));
        assert_eq!(action, InputAction::Edit);
    }

    /// 'o' and 'O' from search-entered logs view return the correct actions.
    #[test]
    fn test_search_logs_subprocess_actions() {
        use crate::types::SearchResult;

        let mut app = App::new();
        app.view_mode = ViewMode::Logs;
        app.entered_from_search = true;
        app.log_lines = vec!["line 1".to_string()];
        app.search_results = vec![SearchResult {
            resource: fake_pod("pod-0"),
            context: "ctx".to_string(),
            resource_type: ResourceType::Pods,
        }];
        app.search_filtered = vec![0];
        app.search_table_state.select(Some(0));

        let action = app.handle_input(key(KeyCode::Char('o')));
        assert_eq!(action, InputAction::OpenLogsInEditor);

        app.view_mode = ViewMode::Logs;
        app.entered_from_search = true;
        let action = app.handle_input(key(KeyCode::Char('O')));
        assert_eq!(action, InputAction::OpenLogsInLess);
    }

    // -----------------------------------------------------------------------
    // Integration test: verify stale events are drained after subprocess
    // -----------------------------------------------------------------------

    /// Simulate the full event flow with the fix: after a subprocess action
    /// is processed, suspend/resume drains stale events so they don't
    /// leak into the kterm event loop.
    #[tokio::test]
    async fn test_stale_events_drained_after_subprocess() {
        let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
        let mut app = app_in_logs_view();

        // 1. User presses 'O' to open less
        tx.send(AppEvent::Key(key(KeyCode::Char('O')))).unwrap();

        // 2. EventHandler's crossterm reader picks up more keystrokes
        //    that were meant for less (before suspend kicks in)
        tx.send(AppEvent::Key(key(KeyCode::Char('q')))).unwrap();
        tx.send(AppEvent::Key(key(KeyCode::Char('j')))).unwrap();
        // A K8s event also arrives during this time
        tx.send(AppEvent::LogLine("new log".to_string())).unwrap();

        // 3. Process the first event — triggers subprocess launch
        if let Some(AppEvent::Key(k)) = rx.recv().await {
            let action = app.handle_input(k);
            assert_eq!(action, InputAction::OpenLogsInLess);
        }

        // 4. The fix: drain stale key events (simulating what
        //    suspend() + resume() does in main.rs)
        let mut kept = Vec::new();
        while let Ok(event) = rx.try_recv() {
            match event {
                AppEvent::Key(_) | AppEvent::Resize(_, _) => {
                    // Discarded by drain_stale_input_events
                }
                other => kept.push(other),
            }
        }

        // 5. Verify: stale 'q' and 'j' were discarded
        assert_eq!(kept.len(), 1);
        assert!(
            matches!(&kept[0], AppEvent::LogLine(s) if s == "new log"),
            "K8s events should be preserved, stale key events discarded"
        );

        // 6. App stays in Logs view — the stale 'q' didn't cause
        //    an unexpected exit
        assert_eq!(app.view_mode, ViewMode::Logs);
    }

    /// Verify that after suspend/resume cycle, the EventHandler can
    /// still deliver new events normally.
    #[tokio::test]
    async fn test_event_handler_functional_after_suspend_resume() {
        let mut handler = EventHandler::new();
        let tx = handler.sender();

        // Simulate a subprocess cycle
        handler.suspend();
        handler.resume();

        // Send a fresh event after resume
        tx.send(AppEvent::K8sError("test".to_string())).unwrap();

        // Should receive it
        let event = handler.next().await;
        assert!(event.is_some());
        assert!(matches!(event.unwrap(), AppEvent::K8sError(s) if s == "test"));
    }

    /// Verify that multiple suspend/resume cycles work correctly.
    #[tokio::test]
    async fn test_multiple_suspend_resume_cycles() {
        let mut handler = EventHandler::new();
        let tx = handler.sender();

        for i in 0..3 {
            // Inject stale key events
            tx.send(AppEvent::Key(key(KeyCode::Char('x')))).unwrap();

            handler.suspend();
            handler.resume();

            // Inject a real event after resume
            tx.send(AppEvent::K8sError(format!("cycle-{}", i)))
                .unwrap();
        }

        // Should only see the K8sError events, not the stale key events
        let mut errors = Vec::new();
        while let Ok(event) = handler.try_recv() {
            match event {
                AppEvent::K8sError(s) => errors.push(s),
                AppEvent::Key(_) => panic!("Stale key event should have been drained"),
                _ => {} // Tick events are fine
            }
        }

        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0], "cycle-0");
        assert_eq!(errors[1], "cycle-1");
        assert_eq!(errors[2], "cycle-2");
    }
}
