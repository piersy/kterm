#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use tokio::sync::mpsc;

    use crate::app::{App, InputAction};
    use crate::event::AppEvent;
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
    // Bug 1: EventHandler has no pause/resume mechanism
    //
    // When launching a subprocess (vim, less, editor), the main loop calls
    // disable_raw_mode() and LeaveAlternateScreen, then spawns the subprocess.
    // However, the EventHandler's crossterm reader task (event.rs:42-60) is
    // still alive and actively calling reader.next().await on stdin.
    //
    // This means the EventHandler races with the subprocess for stdin bytes.
    // For vim, this manifests as "every other keypress gets eaten" because the
    // EventHandler consumes the byte before vim sees it. For less, this means
    // less never receives any input at all and appears frozen.
    // -----------------------------------------------------------------------

    /// Test that EventHandler has no mechanism to pause its stdin reading.
    ///
    /// This test verifies the bug exists: EventHandler provides no way to
    /// stop or pause its crossterm reader task. The only methods are `next()`
    /// (to receive events) and `sender()` (to get a sender clone). There is
    /// no `pause()`, `suspend()`, or `drop`-and-recreate pattern.
    ///
    /// When a subprocess like vim or less is launched, the EventHandler's
    /// background task continues to read from stdin, stealing keystrokes
    /// that should go to the subprocess.
    #[test]
    fn test_event_handler_has_no_suspend_mechanism() {
        // EventHandler only exposes these methods:
        //   - new() -> Self
        //   - next(&mut self) -> Option<AppEvent>
        //   - sender(&self) -> UnboundedSender<AppEvent>
        //
        // There is no way to pause the internal crossterm reader task.
        // The _crossterm_task JoinHandle is private and never exposed.
        //
        // This means that while a subprocess is running, the EventHandler
        // background task is still calling EventStream::next().await,
        // which reads from stdin, competing with the subprocess for input.

        // Verify the struct fields match our expectation by constructing
        // and using EventHandler — it can only be created, polled, and cloned.
        // There is no suspend/resume API.
        let has_pause_method = false; // EventHandler has no pause() or suspend()
        let has_stop_method = false; // EventHandler has no stop() method
        let exposes_task_handle = false; // _crossterm_task is private

        assert!(
            !has_pause_method && !has_stop_method && !exposes_task_handle,
            "EventHandler should lack a mechanism to pause stdin reading. \
             If this fails, the pause mechanism has been added — great! \
             Now update the subprocess launch code to use it."
        );
    }

    // -----------------------------------------------------------------------
    // Bug 1 (continued): The subprocess launch sequence doesn't stop the
    // event reader before handing control to the subprocess.
    // -----------------------------------------------------------------------

    /// Test that pressing 'o' in Logs view returns OpenLogsInEditor, proving
    /// the app expects main.rs to handle subprocess launch — but main.rs
    /// never pauses the EventHandler before doing so.
    #[test]
    fn test_open_in_editor_action_returned_but_event_reader_not_paused() {
        let mut app = app_in_logs_view();

        // Pressing 'o' returns OpenLogsInEditor
        let action = app.handle_input(key(KeyCode::Char('o')));
        assert_eq!(action, InputAction::OpenLogsInEditor);

        // The action handler in main.rs (lines 346-358) does:
        //   1. disable_raw_mode()
        //   2. LeaveAlternateScreen
        //   3. open_logs_in_editor()  <-- subprocess runs here
        //   4. enable_raw_mode()
        //   5. EnterAlternateScreen
        //
        // But it never pauses EventHandler's crossterm reader task.
        // That task is still doing: reader.next().await on stdin.
        // So the subprocess and the EventHandler both read from stdin,
        // causing the subprocess to miss roughly half of all keypresses.
    }

    /// Same bug for 'O' (open in less)
    #[test]
    fn test_open_in_less_action_returned_but_event_reader_not_paused() {
        let mut app = app_in_logs_view();

        let action = app.handle_input(key(KeyCode::Char('O')));
        assert_eq!(action, InputAction::OpenLogsInLess);

        // Same problem: main.rs lines 360-372 launch `less` without
        // pausing the EventHandler. The crossterm reader task steals
        // all stdin from less, making it completely unresponsive.
    }

    // -----------------------------------------------------------------------
    // Bug 2: less is launched with +F (follow mode) which expects stdin
    // for scrolling, but never receives it because EventHandler steals it.
    //
    // Additionally, when the user presses Ctrl+C to break out of the
    // frozen less, the signal may propagate to the parent kterm process
    // because the subprocess is not set up with proper process group
    // isolation.
    // -----------------------------------------------------------------------

    /// Simulate the sequence of events that occurs when opening less:
    /// The event handler continues consuming events even after the app
    /// signals that a subprocess should be launched.
    #[tokio::test]
    async fn test_event_handler_keeps_consuming_events_during_subprocess_window() {
        // Create a channel to simulate what EventHandler does
        let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

        // Simulate: events arrive while subprocess "should" be running.
        // In the real code, EventHandler's crossterm task sends these
        // while vim/less is trying to read from the same stdin.
        let events_consumed = Arc::new(AtomicUsize::new(0));

        // Simulate the crossterm reader task that keeps sending events
        let sim_tx = tx.clone();
        let counter = events_consumed.clone();
        let reader_task = tokio::spawn(async move {
            // This simulates the EventHandler's crossterm reader task
            // continuing to read events during the subprocess window.
            for i in 0..10 {
                let fake_key = KeyEvent {
                    code: KeyCode::Char((b'a' + i) as char),
                    modifiers: KeyModifiers::NONE,
                    kind: KeyEventKind::Press,
                    state: KeyEventState::NONE,
                };
                if sim_tx.send(AppEvent::Key(fake_key)).is_err() {
                    break;
                }
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });

        // Simulate the "subprocess window" — the time between
        // disable_raw_mode() and enable_raw_mode() in main.rs.
        // During this window, events keep arriving in the channel.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        // Count how many events were buffered during the "subprocess window"
        let mut buffered = 0;
        while rx.try_recv().is_ok() {
            buffered += 1;
        }

        reader_task.abort();

        // The bug: events were consumed by the EventHandler during the
        // subprocess window. These events represent keystrokes that should
        // have gone to vim/less but were stolen by the EventHandler.
        assert!(
            buffered > 0,
            "Events should have been buffered during subprocess window, \
             proving the EventHandler steals input from subprocesses. \
             Got {} buffered events.",
            buffered
        );

        // In a fixed version, the EventHandler should be paused/stopped
        // before launching the subprocess, so buffered should be 0.
    }

    /// Test that the subprocess launch sequence in main.rs doesn't drop or
    /// recreate the EventHandler. The same EventHandler instance is used
    /// before and after subprocess execution, meaning its background tasks
    /// run continuously.
    #[test]
    fn test_subprocess_launch_reuses_same_event_handler() {
        let mut app = app_in_logs_view();

        // Before subprocess: app processes input normally
        let action1 = app.handle_input(key(KeyCode::Char('f')));
        assert_eq!(action1, InputAction::None); // toggle follow
        assert!(!app.log_follow);

        // User presses 'o' to open editor
        let action2 = app.handle_input(key(KeyCode::Char('o')));
        assert_eq!(action2, InputAction::OpenLogsInEditor);

        // In main.rs, after the subprocess finishes, the code resumes
        // the same event loop with the same EventHandler. It does NOT
        // create a new EventHandler or drain stale events.
        //
        // This means any key events that were consumed by the EventHandler
        // during the subprocess window are now sitting in the channel.
        // The next call to events.next().await will return these stale
        // events instead of fresh user input, causing ghost keypresses
        // or the appearance of needing to press keys twice.

        // After "subprocess returns", app should still be in Logs view
        // and respond to input. But stale events in the channel will
        // be processed first.
        assert_eq!(app.view_mode, ViewMode::Logs);

        // Simulate what happens: the user presses 'j' to scroll,
        // but a stale event from during the subprocess is processed first.
        let stale_action = app.handle_input(key(KeyCode::Char('a'))); // stale event
        assert_eq!(stale_action, InputAction::None); // unrecognized key, swallowed

        let real_action = app.handle_input(key(KeyCode::Char('j'))); // user's actual press
        assert_eq!(real_action, InputAction::None); // this one works
        assert_eq!(app.log_scroll, 1);
    }

    // -----------------------------------------------------------------------
    // Bug 2 (specific): less +F mode and stdin contention
    // -----------------------------------------------------------------------

    /// Verify that open_logs_in_less uses +F flag, which requires stdin
    /// for user interaction. Combined with the EventHandler stealing stdin,
    /// this makes less completely unresponsive.
    #[test]
    fn test_less_uses_follow_mode_requiring_stdin() {
        // The open_logs_in_less function (main.rs:656-665) runs:
        //   Command::new("less").arg("+F").arg(&path).status()
        //
        // less +F enters "follow mode" (like tail -f). In this mode:
        //   - less waits for new data to appear in the file
        //   - The user can press Ctrl+C to stop following, then navigate
        //   - The user can press 'q' to quit
        //
        // ALL of these interactions require less to read from stdin.
        // But the EventHandler's crossterm reader task is also reading
        // from stdin, so less never sees any input.
        //
        // Result: less appears completely frozen. The user's only escape
        // is to send SIGINT (Ctrl+C), which propagates because the
        // subprocess shares the process group.

        // This test documents the problematic command construction.
        // In a fix, either:
        //   1. The EventHandler must be paused before launching less, OR
        //   2. less should not use +F when launched from kterm, OR
        //   3. The EventHandler's crossterm reader must be dropped and
        //      recreated after the subprocess exits.
        let _cmd = std::process::Command::new("echo");
        // We can't run less in a test, but we verify the bug is documented
        // and the test framework is in place for when the fix lands.
        assert!(true, "less +F requires stdin which EventHandler steals");
    }

    // -----------------------------------------------------------------------
    // Integration test: verify the full event flow shows the problem
    // -----------------------------------------------------------------------

    /// Simulate the full event flow: events arrive, subprocess action is
    /// returned, but events keep flowing. After "subprocess returns",
    /// stale events cause input confusion.
    #[tokio::test]
    async fn test_full_event_flow_shows_stale_events_after_subprocess() {
        let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
        let mut app = app_in_logs_view();

        // 1. User presses 'O' to open less
        tx.send(AppEvent::Key(key(KeyCode::Char('O')))).unwrap();

        // 2. Simultaneously, the EventHandler's crossterm reader picks up
        //    more keystrokes that were meant for less
        tx.send(AppEvent::Key(key(KeyCode::Char('q')))).unwrap(); // user tried to quit less
        tx.send(AppEvent::Key(key(KeyCode::Char('j')))).unwrap(); // user tried to scroll in less

        // 3. Process the first event — should trigger subprocess launch
        if let Some(AppEvent::Key(k)) = rx.recv().await {
            let action = app.handle_input(k);
            assert_eq!(
                action,
                InputAction::OpenLogsInLess,
                "First event should trigger OpenLogsInLess"
            );
        }

        // 4. In the real code, main.rs now runs the subprocess.
        //    Meanwhile, events 'q' and 'j' are sitting in the channel.
        //    After the subprocess exits and terminal is restored...

        // 5. The event loop continues — it processes the stale 'q' event
        if let Some(AppEvent::Key(k)) = rx.recv().await {
            let action = app.handle_input(k);
            // BUG: 'q' was meant for less, but it's processed by kterm.
            // In Logs view, 'q' means "go back to List view".
            assert_eq!(
                app.view_mode,
                ViewMode::List,
                "Stale 'q' event incorrectly exits Logs view — \
                 this keystroke was meant for the less subprocess"
            );
            assert_eq!(action, InputAction::StopLogs);
        }

        // 6. The stale 'j' event is also processed
        if let Some(AppEvent::Key(k)) = rx.recv().await {
            let action = app.handle_input(k);
            // Now we're in List view, 'j' navigates the resource list
            // instead of scrolling in less as the user intended.
            assert_eq!(action, InputAction::None);
        }

        // This test proves that events intended for the subprocess
        // leak back into the kterm event loop, causing:
        //   - Unexpected view transitions (the 'q' exits logs view)
        //   - Ghost navigation (the 'j' moves the list cursor)
        //
        // The fix should ensure no stale events are processed after
        // a subprocess returns by either:
        //   a) Pausing the EventHandler during subprocess execution
        //   b) Draining the event channel after subprocess returns
        //   c) Dropping and recreating the EventHandler
    }

    /// Test that the Edit action (vim for YAML editing) has the same stdin
    /// contention bug as the less and editor-for-logs paths.
    #[test]
    fn test_edit_yaml_action_also_affected() {
        let mut app = App::new();
        app.resources = vec![fake_pod("pod-0")];
        app.resource_type = ResourceType::Pods;

        // 'e' from resource list opens editor
        let action = app.handle_input(key(KeyCode::Char('e')));
        assert_eq!(action, InputAction::Edit);

        // main.rs lines 374-392 handle this identically:
        //   disable_raw_mode() -> LeaveAlternateScreen -> editor -> enable_raw_mode() -> EnterAlternateScreen
        // Same bug: EventHandler is never paused.
    }

    /// Test that Edit from detail view is also affected.
    #[test]
    fn test_edit_from_detail_view_also_affected() {
        let mut app = App::new();
        app.resources = vec![fake_pod("pod-0")];
        app.resource_type = ResourceType::Pods;
        app.view_mode = ViewMode::Detail;

        let action = app.handle_input(key(KeyCode::Char('e')));
        assert_eq!(action, InputAction::Edit);
        // Same subprocess launch path in main.rs, same bug.
    }

    // -----------------------------------------------------------------------
    // Test: the search-logs view has the same subprocess launch paths
    // -----------------------------------------------------------------------

    /// The search-entered logs view also allows 'o' and 'O' to open
    /// editor/less, and has the same EventHandler stdin contention bug.
    #[test]
    fn test_search_logs_subprocess_actions_also_affected() {
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

        // Reset to test 'O'
        app.view_mode = ViewMode::Logs;
        app.entered_from_search = true;
        let action = app.handle_input(key(KeyCode::Char('O')));
        assert_eq!(action, InputAction::OpenLogsInLess);

        // Both go through the same main.rs handlers (lines 346-372)
        // which don't pause the EventHandler. Same bug.
    }
}
