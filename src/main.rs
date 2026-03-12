mod app;
#[cfg(test)]
mod app_test;
mod event;
mod k8s;
mod types;
mod ui;
#[cfg(test)]
mod ui_test;
#[cfg(test)]
mod subprocess_test;

use std::collections::HashMap;
use std::io;

use anyhow::Result;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::{App, InputAction};
use event::{AppEvent, EventHandler};

#[tokio::main]
async fn main() -> Result<()> {
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal).await;

    // Terminal teardown
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }

    Ok(())
}

/// Abort all current watcher handles and clear the map.
fn abort_all_watchers(
    watcher_handles: &mut HashMap<types::ResourceType, tokio::task::JoinHandle<()>>,
) {
    for (_, h) in watcher_handles.drain() {
        h.abort();
    }
}

/// Start watchers for the currently selected resource types.
fn start_watchers(
    app: &App,
    k8s_manager: &std::sync::Arc<tokio::sync::Mutex<Option<k8s::client::K8sManager>>>,
    tx: &tokio::sync::mpsc::UnboundedSender<AppEvent>,
    watcher_handles: &mut HashMap<types::ResourceType, tokio::task::JoinHandle<()>>,
) {
    let ns = app.current_namespace().to_string();

    for &rt in &app.selected_resource_types {
        let mgr = k8s_manager.clone();
        let action_tx = tx.clone();
        let ns = ns.clone();

        let handle = tokio::spawn(async move {
            let guard = mgr.lock().await;
            if let Some(ref manager) = *guard {
                let client = manager.client.clone();
                drop(guard);
                if let Err(e) =
                    k8s::resources::watch_resources(client, &ns, rt, action_tx.clone()).await
                {
                    let _ = action_tx.send(AppEvent::K8sError(format!("Watch error: {}", e)));
                }
            }
        });
        watcher_handles.insert(rt, handle);
    }
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    let mut events = EventHandler::new();
    let tx = events.sender();

    let k8s_manager: std::sync::Arc<tokio::sync::Mutex<Option<k8s::client::K8sManager>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(None));

    // Try to connect to Kubernetes
    app.loading = true;
    let k8s_tx = tx.clone();
    let init_mgr = k8s_manager.clone();
    tokio::spawn(async move {
        match k8s::client::K8sManager::new().await {
            Ok(manager) => {
                let contexts = manager.context_names();
                let current = manager.current_context.clone();
                let current_namespace = manager.current_namespace();

                match manager.list_namespaces().await {
                    Ok(namespaces) => {
                        let _ = k8s_tx.send(AppEvent::NamespacesLoaded(namespaces));
                    }
                    Err(e) => {
                        let _ = k8s_tx.send(AppEvent::K8sError(format!(
                            "Failed to list namespaces: {}",
                            e
                        )));
                        let _ =
                            k8s_tx.send(AppEvent::NamespacesLoaded(vec!["default".to_string()]));
                    }
                }

                *init_mgr.lock().await = Some(manager);

                let _ = k8s_tx.send(AppEvent::ContextsLoaded {
                    contexts,
                    current,
                    current_namespace,
                });
            }
            Err(e) => {
                let _ = k8s_tx.send(AppEvent::K8sError(format!(
                    "Failed to connect to Kubernetes: {}. Running in offline mode.",
                    e
                )));
                let _ = k8s_tx.send(AppEvent::NamespacesLoaded(vec!["default".to_string()]));
            }
        }
    });

    // Track watcher tasks per resource type
    let mut watcher_handles: HashMap<types::ResourceType, tokio::task::JoinHandle<()>> =
        HashMap::new();

    loop {
        terminal.draw(|f| ui::render(f, &mut app))?;

        let Some(event) = events.next().await else {
            break;
        };

        match event {
            AppEvent::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                let action = app.handle_input(key);

                match action {
                    InputAction::ContextChanged => {
                        let context_name = app.current_context().to_string();
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();

                        abort_all_watchers(&mut watcher_handles);
                        app.loading = true;
                        app.resources_by_type.clear();

                        let selected_types = app.selected_resource_types.clone();
                        let ns_pref = app.current_namespace().to_string();

                        let handle = tokio::spawn(async move {
                            let mut guard = mgr.lock().await;
                            if let Some(ref mut manager) = *guard {
                                if let Err(e) =
                                    manager.switch_context(&context_name).await
                                {
                                    let _ = action_tx.send(AppEvent::K8sError(format!(
                                        "Failed to switch context: {}",
                                        e
                                    )));
                                    return;
                                }
                                match manager.list_namespaces().await {
                                    Ok(namespaces) => {
                                        let _ = action_tx
                                            .send(AppEvent::NamespacesLoaded(namespaces));
                                    }
                                    Err(e) => {
                                        let _ = action_tx.send(AppEvent::K8sError(format!(
                                            "Failed to list namespaces: {}",
                                            e
                                        )));
                                    }
                                }
                                let client = manager.client.clone();
                                drop(guard);
                                // Count resources
                                let count_tx = action_tx.clone();
                                let count_client = client.clone();
                                let count_ns = ns_pref.clone();
                                tokio::spawn(async move {
                                    let counts = k8s::resources::count_all_resources(
                                        count_client,
                                        &count_ns,
                                    )
                                    .await;
                                    let _ =
                                        count_tx.send(AppEvent::ResourceCountsLoaded(counts));
                                });
                                // Start watching all selected types
                                for rt in selected_types {
                                    let c = client.clone();
                                    let t = action_tx.clone();
                                    let n = ns_pref.clone();
                                    tokio::spawn(async move {
                                        if let Err(e) =
                                            k8s::resources::watch_resources(c, &n, rt, t.clone())
                                                .await
                                        {
                                            let _ = t.send(AppEvent::K8sError(format!(
                                                "Watch error: {}",
                                                e
                                            )));
                                        }
                                    });
                                }
                            }
                        });
                        // We track via the spawned sub-tasks, but keep the main handle too
                        watcher_handles.insert(types::ResourceType::Pods, handle);
                    }
                    InputAction::NamespaceChanged => {
                        abort_all_watchers(&mut watcher_handles);
                        app.loading = true;
                        app.resources_by_type.clear();
                        app.resource_counts.clear();
                        app.table_state.select(Some(0));

                        // Start count fetch
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();
                        let ns = app.current_namespace().to_string();
                        {
                            let count_mgr = mgr.clone();
                            let count_tx = action_tx.clone();
                            let count_ns = ns.clone();
                            tokio::spawn(async move {
                                let guard = count_mgr.lock().await;
                                if let Some(ref manager) = *guard {
                                    let client = manager.client.clone();
                                    drop(guard);
                                    let counts = k8s::resources::count_all_resources(
                                        client, &count_ns,
                                    )
                                    .await;
                                    let _ =
                                        count_tx.send(AppEvent::ResourceCountsLoaded(counts));
                                }
                            });
                        }

                        start_watchers(&app, &k8s_manager, &tx, &mut watcher_handles);
                    }
                    InputAction::ResourceTypeChanged => {
                        abort_all_watchers(&mut watcher_handles);
                        app.loading = true;
                        app.resources_by_type.clear();
                        app.table_state.select(Some(0));

                        start_watchers(&app, &k8s_manager, &tx, &mut watcher_handles);
                    }
                    InputAction::Describe => {
                        let (name, ns, rt) = {
                            if let Some((res, rt)) = app.selected_resource() {
                                (res.name.clone(), res.namespace.clone(), rt)
                            } else {
                                continue;
                            }
                        };
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();
                        let ns = if ns.is_empty() {
                            app.current_namespace().to_string()
                        } else {
                            ns
                        };

                        app.loading = true;
                        app.detail_text.clear();

                        tokio::spawn(async move {
                            let guard = mgr.lock().await;
                            if let Some(ref manager) = *guard {
                                let client = manager.client.clone();
                                drop(guard);
                                match k8s::resources::describe_resource(
                                    client, &ns, &name, rt,
                                )
                                .await
                                {
                                    Ok(desc) => {
                                        let _ =
                                            action_tx.send(AppEvent::DetailLoaded(desc));
                                    }
                                    Err(e) => {
                                        let _ = action_tx.send(AppEvent::K8sError(
                                            format!("Describe error: {}", e),
                                        ));
                                    }
                                }
                            }
                        });
                    }
                    InputAction::StreamLogs => {
                        let name = app.selected_resource_name().unwrap_or_default();
                        let ns = app.current_namespace().to_string();
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();

                        app.loading = true;

                        tokio::spawn(async move {
                            let guard = mgr.lock().await;
                            if let Some(ref manager) = *guard {
                                let client = manager.client.clone();
                                drop(guard);
                                if let Err(e) = k8s::logs::stream_pod_logs(
                                    client,
                                    &ns,
                                    &name,
                                    None,
                                    action_tx.clone(),
                                )
                                .await
                                {
                                    let _ = action_tx.send(AppEvent::K8sError(format!(
                                        "Log stream error: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    InputAction::StopLogs => {}
                    InputAction::Delete => {
                        let (name, rt) = {
                            if let Some((res, rt)) = app.selected_resource() {
                                (res.name.clone(), rt)
                            } else {
                                continue;
                            }
                        };
                        let ns = app.current_namespace().to_string();
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();

                        tokio::spawn(async move {
                            let guard = mgr.lock().await;
                            if let Some(ref manager) = *guard {
                                let client = manager.client.clone();
                                drop(guard);
                                if let Err(e) =
                                    k8s::actions::delete_resource(client, &ns, &name, rt)
                                        .await
                                {
                                    let _ = action_tx.send(AppEvent::K8sError(format!(
                                        "Delete error: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    InputAction::Restart => {
                        let (name, rt) = {
                            if let Some((res, rt)) = app.selected_resource() {
                                (res.name.clone(), rt)
                            } else {
                                continue;
                            }
                        };
                        let ns = app.current_namespace().to_string();
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();

                        tokio::spawn(async move {
                            let guard = mgr.lock().await;
                            if let Some(ref manager) = *guard {
                                let client = manager.client.clone();
                                drop(guard);
                                if let Err(e) =
                                    k8s::actions::restart_resource(client, &ns, &name, rt)
                                        .await
                                {
                                    let _ = action_tx.send(AppEvent::K8sError(format!(
                                        "Restart error: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    InputAction::OpenLogsInEditor => {
                        if !app.log_lines.is_empty() {
                            events.suspend();
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                            let _ = open_logs_in_editor(&app.log_lines);

                            enable_raw_mode()?;
                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                            terminal.clear()?;
                            events.resume();
                        }
                    }
                    InputAction::OpenLogsInLess => {
                        if !app.log_lines.is_empty() {
                            let client_and_pod = if app.entered_from_search {
                                if let Some(result) = app.selected_search_result().cloned() {
                                    let client = k8s::client::K8sManager::client_for_context(
                                        &result.context,
                                    )
                                    .await
                                    .ok();
                                    client.map(|c| {
                                        (
                                            c,
                                            result.resource.namespace.clone(),
                                            result.resource.name.clone(),
                                        )
                                    })
                                } else {
                                    None
                                }
                            } else {
                                let guard = k8s_manager.lock().await;
                                guard.as_ref().map(|mgr| {
                                    (
                                        mgr.client.clone(),
                                        app.current_namespace().to_string(),
                                        app.selected_resource_name().unwrap_or_default(),
                                    )
                                })
                            };

                            events.suspend();
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                            let cleanup =
                                if let Some((client, ns, pod_name)) = client_and_pod {
                                    open_logs_in_less(
                                        &app.log_lines,
                                        client,
                                        ns,
                                        pod_name,
                                        None,
                                    )
                                    .ok()
                                } else {
                                    None
                                };

                            enable_raw_mode()?;
                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                            terminal.clear()?;
                            events.resume();

                            if let Some(c) = cleanup {
                                c.finish_in_background();
                            }
                        }
                    }
                    InputAction::Edit => {
                        if let Some((resource, rt)) = app.selected_resource() {
                            let yaml = resource.raw_yaml.clone();
                            let name = resource.name.clone();
                            let ns = app.current_namespace().to_string();
                            let mgr = k8s_manager.clone();
                            let action_tx = tx.clone();

                            events.suspend();
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                            let edited = edit_yaml_in_editor(&yaml);

                            enable_raw_mode()?;
                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                            terminal.clear()?;
                            events.resume();

                            if let Ok(Some(new_yaml)) = edited {
                                tokio::spawn(async move {
                                    let guard = mgr.lock().await;
                                    if let Some(ref manager) = *guard {
                                        let client = manager.client.clone();
                                        drop(guard);
                                        if let Err(e) = k8s::actions::apply_yaml(
                                            client, &ns, &name, rt, &new_yaml,
                                        )
                                        .await
                                        {
                                            let _ = action_tx.send(AppEvent::K8sError(
                                                format!("Apply error: {}", e),
                                            ));
                                        }
                                    }
                                });
                            }
                        }
                    }
                    InputAction::StartSearch => {
                        let contexts = app.contexts.clone();
                        app.search_contexts_total = contexts.len();
                        app.search_contexts_done = 0;

                        for context in contexts {
                            let ctx = context.clone();
                            let search_tx = tx.clone();
                            tokio::spawn(async move {
                                match k8s::client::K8sManager::client_for_context(&ctx).await
                                {
                                    Ok(client) => {
                                        for rt in types::ResourceType::ALL.iter() {
                                            let rt = *rt;
                                            match k8s::resources::list_all_resources(
                                                client.clone(),
                                                rt,
                                            )
                                            .await
                                            {
                                                Ok(items) => {
                                                    let _ = search_tx.send(
                                                        AppEvent::SearchResultsBatch {
                                                            context: ctx.clone(),
                                                            resource_type: rt,
                                                            items,
                                                        },
                                                    );
                                                }
                                                Err(e) => {
                                                    let _ = search_tx.send(
                                                        AppEvent::K8sError(format!(
                                                            "Search {}/{}: {}",
                                                            ctx, rt, e
                                                        )),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = search_tx.send(AppEvent::K8sError(
                                            format!("Connect to {}: {}", ctx, e),
                                        ));
                                    }
                                }
                                let _ = search_tx
                                    .send(AppEvent::SearchScanComplete(ctx));
                            });
                        }
                    }
                    InputAction::SearchDescribe => {
                        if let Some(result) = app.selected_search_result().cloned() {
                            let action_tx = tx.clone();
                            app.loading = true;

                            tokio::spawn(async move {
                                match k8s::client::K8sManager::client_for_context(
                                    &result.context,
                                )
                                .await
                                {
                                    Ok(client) => {
                                        match k8s::resources::describe_resource(
                                            client,
                                            &result.resource.namespace,
                                            &result.resource.name,
                                            result.resource_type,
                                        )
                                        .await
                                        {
                                            Ok(desc) => {
                                                let _ = action_tx
                                                    .send(AppEvent::DetailLoaded(desc));
                                            }
                                            Err(e) => {
                                                let _ = action_tx.send(
                                                    AppEvent::K8sError(format!(
                                                        "Describe error: {}",
                                                        e
                                                    )),
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = action_tx.send(AppEvent::K8sError(
                                            format!(
                                                "Connect to {}: {}",
                                                result.context, e
                                            ),
                                        ));
                                    }
                                }
                            });
                        }
                    }
                    InputAction::SearchStreamLogs => {
                        if let Some(result) = app.selected_search_result().cloned() {
                            let action_tx = tx.clone();
                            app.loading = true;

                            tokio::spawn(async move {
                                match k8s::client::K8sManager::client_for_context(
                                    &result.context,
                                )
                                .await
                                {
                                    Ok(client) => {
                                        if let Err(e) = k8s::logs::stream_pod_logs(
                                            client,
                                            &result.resource.namespace,
                                            &result.resource.name,
                                            None,
                                            action_tx.clone(),
                                        )
                                        .await
                                        {
                                            let _ = action_tx.send(AppEvent::K8sError(
                                                format!("Log stream error: {}", e),
                                            ));
                                        }
                                    }
                                    Err(e) => {
                                        let _ = action_tx.send(AppEvent::K8sError(
                                            format!(
                                                "Connect to {}: {}",
                                                result.context, e
                                            ),
                                        ));
                                    }
                                }
                            });
                        }
                    }
                    InputAction::None => {}
                }
            }
            AppEvent::Tick => {
                app.handle_tick();
            }
            AppEvent::Resize(_, _) => {}
            AppEvent::ResourcesUpdated(items) => {
                // Determine which resource type these items belong to.
                // The watcher sends items for a specific type, but the event
                // doesn't carry the type. We infer from the watcher setup:
                // each watcher is for one type, and sends updates for that type.
                // Since we can have multiple watchers, we need the event to carry the type.
                // For now, store under all selected types if single, or use the event's type.
                // TODO: The event should carry the resource type. For now, if single type
                // selected, store there. If multiple, this won't work correctly without
                // the type in the event. We'll fix this by adding the type to the event.

                // Actually, let's check: the watch_resources function takes a ResourceType
                // and we need to propagate it. Let's store resources for the primary type
                // for backwards compat, but we need to fix this properly.
                // The real fix is in event.rs - add ResourceType to ResourcesUpdated.
                // For now, we'll handle it via the new ResourcesUpdatedForType event.

                // Legacy fallback: store under primary type
                let rt = app.primary_resource_type();
                app.resources_by_type.insert(rt, items);
                app.loading = false;
                let rows = app.display_rows();
                let len = rows.len();
                if len > 0 {
                    if let Some(selected) = app.table_state.selected() {
                        if selected >= len {
                            app.table_state.select(Some(len - 1));
                        }
                    }
                }
            }
            AppEvent::ResourcesUpdatedForType(rt, items) => {
                app.resources_by_type.insert(rt, items);
                app.loading = false;
                let rows = app.display_rows();
                let len = rows.len();
                if len > 0 {
                    if let Some(selected) = app.table_state.selected() {
                        if selected >= len {
                            app.table_state.select(Some(len - 1));
                        }
                    }
                }
            }
            AppEvent::NamespacesLoaded(namespaces) => {
                app.namespaces = namespaces;
                if let Some(ref pref) = app.preferred_namespace {
                    if let Some(idx) = app.namespaces.iter().position(|n| n == pref) {
                        app.selected_namespaces.clear();
                        app.selected_namespaces.insert(idx);
                    } else {
                        app.selected_namespaces.clear();
                        app.selected_namespaces.insert(0);
                    }
                } else {
                    app.selected_namespaces.clear();
                    app.selected_namespaces.insert(0);
                }
                app.loading = false;
                if let types::Focus::Selector(types::SelectorTarget::Namespace) = app.focus {
                    app.update_dropdown_filter();
                }
            }
            AppEvent::DetailLoaded(text) => {
                app.detail_text = text;
                app.loading = false;
            }
            AppEvent::LogLine(line) => {
                app.log_lines.push(line);
                app.loading = false;
            }
            AppEvent::LogStreamEnded => {
                app.loading = false;
            }
            AppEvent::ContextsLoaded {
                contexts,
                current,
                current_namespace,
            } => {
                app.contexts = contexts;
                if let Some(idx) = app.contexts.iter().position(|c| c == &current) {
                    app.selected_contexts.clear();
                    app.selected_contexts.insert(idx);
                }
                if let types::Focus::Selector(types::SelectorTarget::Context) = app.focus {
                    app.update_dropdown_filter();
                }
                app.preferred_namespace = Some(current_namespace.clone());
                if let Some(idx) =
                    app.namespaces.iter().position(|n| n == &current_namespace)
                {
                    app.selected_namespaces.clear();
                    app.selected_namespaces.insert(idx);
                }

                // Start initial resource watchers
                abort_all_watchers(&mut watcher_handles);
                let mgr = k8s_manager.clone();
                let action_tx = tx.clone();
                let ns = app.current_namespace().to_string();
                let selected_types = app.selected_resource_types.clone();
                let handle = tokio::spawn(async move {
                    let guard = mgr.lock().await;
                    if let Some(ref manager) = *guard {
                        let client = manager.client.clone();
                        drop(guard);
                        // Count resources
                        let count_tx = action_tx.clone();
                        let count_client = client.clone();
                        let count_ns = ns.clone();
                        tokio::spawn(async move {
                            let counts = k8s::resources::count_all_resources(
                                count_client,
                                &count_ns,
                            )
                            .await;
                            let _ = count_tx.send(AppEvent::ResourceCountsLoaded(counts));
                        });
                        // Start watching all selected types
                        for rt in selected_types {
                            let c = client.clone();
                            let t = action_tx.clone();
                            let n = ns.clone();
                            tokio::spawn(async move {
                                if let Err(e) =
                                    k8s::resources::watch_resources(c, &n, rt, t.clone()).await
                                {
                                    let _ = t.send(AppEvent::K8sError(format!(
                                        "Watch error: {}",
                                        e
                                    )));
                                }
                            });
                        }
                    }
                });
                watcher_handles.insert(types::ResourceType::Pods, handle);
            }
            AppEvent::ResourceCountsLoaded(counts) => {
                app.resource_counts = counts;
                if let types::Focus::Selector(types::SelectorTarget::ResourceType) = app.focus
                {
                    app.update_dropdown_filter();
                }
            }
            AppEvent::K8sError(msg) => {
                app.set_error(msg);
                app.loading = false;
            }
            AppEvent::SearchResultsBatch {
                context,
                resource_type,
                items,
            } => {
                if app.view_mode == types::ViewMode::Search {
                    for item in items {
                        app.search_results.push(types::SearchResult {
                            resource: item,
                            context: context.clone(),
                            resource_type,
                        });
                    }
                    app.update_search_filter();
                }
            }
            AppEvent::SearchScanComplete(_context) => {
                if app.view_mode == types::ViewMode::Search {
                    app.search_contexts_done += 1;
                    if app.search_contexts_done >= app.search_contexts_total {
                        app.search_loading = false;
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

fn write_logs_to_tempfile(log_lines: &[String]) -> Result<std::path::PathBuf> {
    use std::io::Write;

    let mut tmp = tempfile::Builder::new()
        .prefix("kterm-logs-")
        .suffix(".log")
        .tempfile()?;
    for line in log_lines {
        writeln!(tmp, "{}", line)?;
    }
    tmp.flush()?;
    let (_, path) = tmp.keep()?;
    Ok(path)
}

fn open_logs_in_editor(log_lines: &[String]) -> Result<()> {
    let path = write_logs_to_tempfile(log_lines)?;
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    std::process::Command::new(&editor)
        .arg(&path)
        .status()?;

    let _ = std::fs::remove_file(&path);
    Ok(())
}

struct LessCleanup {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    writer_handle: Option<std::thread::JoinHandle<()>>,
    path: std::path::PathBuf,
}

impl LessCleanup {
    fn finish_in_background(self) {
        std::thread::spawn(move || {
            self.stop
                .store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(h) = self.writer_handle {
                let _ = h.join();
            }
            let _ = std::fs::remove_file(&self.path);
        });
    }
}

fn open_logs_in_less(
    log_lines: &[String],
    client: kube::Client,
    namespace: String,
    pod_name: String,
    container: Option<String>,
) -> Result<LessCleanup> {
    use std::io::Write;

    let path = write_logs_to_tempfile(log_lines)?;

    let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_flag = stop.clone();

    let writer_handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for log writer");

        rt.block_on(async {
            let api: kube::Api<k8s_openapi::api::core::v1::Pod> =
                kube::Api::namespaced(client, &namespace);

            let mut params = kube::api::LogParams {
                follow: true,
                since_seconds: Some(1),
                ..Default::default()
            };
            if let Some(c) = container {
                params.container = Some(c);
            }

            let stream = match api.log_stream(&pod_name, &params).await {
                Ok(s) => s,
                Err(_) => return,
            };

            use futures::AsyncBufReadExt;
            use futures::TryStreamExt;
            let mut lines = stream.lines();

            while let Ok(Some(line)) = lines.try_next().await {
                if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                if writeln!(file, "{}", line).is_err() {
                    break;
                }
                let _ = file.flush();
            }
        });
    });

    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }

    std::process::Command::new("less")
        .arg("+F")
        .arg(&path)
        .status()?;

    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_DFL);
    }

    Ok(LessCleanup {
        stop,
        writer_handle: Some(writer_handle),
        path,
    })
}

fn edit_yaml_in_editor(yaml: &str) -> Result<Option<String>> {
    use std::io::Write;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(yaml.as_bytes())?;
    tmp.flush()?;

    let path = tmp.path().to_owned();
    let status = std::process::Command::new(&editor).arg(&path).status()?;

    if !status.success() {
        return Ok(None);
    }

    let new_content = std::fs::read_to_string(&path)?;
    if new_content == yaml {
        return Ok(None);
    }

    Ok(Some(new_content))
}
