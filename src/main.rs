mod app;
#[cfg(test)]
mod app_test;
mod event;
mod k8s;
mod logging;
mod types;
mod ui;
#[cfg(test)]
mod ui_test;
#[cfg(test)]
mod subprocess_test;

use std::io;

use anyhow::{Context, Result};
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
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

    // Terminal teardown — clear the alternate screen so it doesn't leak into scrollback
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), Clear(ClearType::All), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }

    Ok(())
}

/// Abort all current watcher handles and clear the list.
fn abort_all_watchers(
    watcher_handles: &mut Vec<tokio::task::JoinHandle<()>>,
    active_watch_types: &mut std::collections::HashSet<types::ResourceType>,
) {
    for h in watcher_handles.drain(..) {
        h.abort();
    }
    active_watch_types.clear();
}

/// Start watchers for the currently selected resource types.
/// All spawned tasks are tracked in watcher_handles so they can be
/// properly aborted on context/namespace/type changes.
fn start_watchers(
    app: &App,
    k8s_manager: &std::sync::Arc<tokio::sync::Mutex<Option<k8s::client::K8sManager>>>,
    tx: &tokio::sync::mpsc::UnboundedSender<AppEvent>,
    watcher_handles: &mut Vec<tokio::task::JoinHandle<()>>,
    active_watch_types: &mut std::collections::HashSet<types::ResourceType>,
) {
    let ns = app.current_namespace().to_string();
    let generation = app.generation;
    let context_name = app.current_context().to_string();

    for &rt in &app.selected_resource_types {
        // Skip if a watcher for this type is already running
        if !active_watch_types.insert(rt) {
            continue;
        }
        let mgr = k8s_manager.clone();
        let action_tx = tx.clone();
        let ns = ns.clone();
        let ctx = context_name.clone();

        let handle = tokio::spawn(async move {
            let guard = mgr.lock().await;
            if let Some(ref manager) = *guard {
                let client = manager.client.clone();
                drop(guard);
                if let Err(e) =
                    k8s::resources::watch_resources(
                        client,
                        &ns,
                        rt,
                        generation,
                        action_tx.clone(),
                    )
                    .await
                {
                    // Mark cluster unreachable so it's excluded from future searches
                    event::send_event(
                        &action_tx,
                        AppEvent::ClusterProbeResult {
                            context: ctx,
                            reachable: false,
                        },
                    );
                    event::send_event(
                        &action_tx,
                        AppEvent::K8sError(format!("Watch error: {}", e)),
                    );
                }
            }
        });
        watcher_handles.push(handle);
    }
}

/// Spawn a resource count fetch task.
///
/// Returns the spawned [`JoinHandle`] so the caller can store it separately
/// from watcher handles. Count fetches are namespace-scoped (not
/// type-scoped) and therefore must survive resource-type changes — they are
/// only aborted on context/namespace changes.
///
/// The returned event carries the `(context, namespace)` the counts were
/// fetched for so stale results from a previous cluster/namespace can be
/// discarded by the main loop.
fn start_count_fetch(
    app: &App,
    k8s_manager: &std::sync::Arc<tokio::sync::Mutex<Option<k8s::client::K8sManager>>>,
    tx: &tokio::sync::mpsc::UnboundedSender<AppEvent>,
    ns: &str,
) -> tokio::task::JoinHandle<()> {
    let mgr = k8s_manager.clone();
    let count_tx = tx.clone();
    let count_ns = ns.to_string();
    let context = app.current_context().to_string();
    tokio::spawn(async move {
        let guard = mgr.lock().await;
        if let Some(ref manager) = *guard {
            let client = manager.client.clone();
            drop(guard);
            let counts = k8s::resources::count_all_resources(client, &count_ns).await;
            event::send_event(
                &count_tx,
                AppEvent::ResourceCountsLoaded {
                    counts,
                    context,
                    namespace: count_ns,
                },
            );
        }
    })
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    logging::init();
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

                let current_reachable = match manager.list_namespaces().await {
                    Ok(namespaces) => {
                        event::send_event(&k8s_tx,AppEvent::NamespacesLoaded(namespaces));
                        true
                    }
                    Err(_) => {
                        // Don't show an error popup here — the probe system will
                        // show "all clusters unreachable" once all probes complete.
                        event::send_event(&k8s_tx, AppEvent::NamespacesLoaded(vec!["default".to_string()]));
                        false
                    }
                };

                *init_mgr.lock().await = Some(manager);

                event::send_event(&k8s_tx,AppEvent::ContextsLoaded {
                    contexts,
                    current,
                    current_namespace,
                    current_reachable,
                });
            }
            Err(e) => {
                // Try to at least read the kubeconfig for context names
                let contexts = kube::config::Kubeconfig::read()
                    .ok()
                    .map(|kc| kc.contexts.iter().map(|c| c.name.clone()).collect::<Vec<_>>());

                if let Some(contexts) = contexts {
                    if !contexts.is_empty() {
                        let current = contexts[0].clone();
                        event::send_event(&k8s_tx, AppEvent::NamespacesLoaded(vec!["default".to_string()]));
                        event::send_event(&k8s_tx, AppEvent::ContextsLoaded {
                            contexts,
                            current,
                            current_namespace: "default".to_string(),
                            current_reachable: false,
                        });
                    } else {
                        event::send_event(&k8s_tx,AppEvent::K8sError(format!(
                            "Failed to connect to Kubernetes: {}. Running in offline mode.",
                            e
                        )));
                        event::send_event(&k8s_tx,AppEvent::NamespacesLoaded(vec!["default".to_string()]));
                    }
                } else {
                    event::send_event(&k8s_tx,AppEvent::K8sError(format!(
                        "Failed to connect to Kubernetes: {}. Running in offline mode.",
                        e
                    )));
                    event::send_event(&k8s_tx,AppEvent::NamespacesLoaded(vec!["default".to_string()]));
                }
            }
        }
    });

    // Track watcher tasks (aborted on context/namespace/type changes) and
    // the count-fetch task (aborted only on context/namespace changes —
    // counts are namespace-scoped and must survive type changes).
    let mut watcher_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let mut active_watch_types: std::collections::HashSet<types::ResourceType> =
        std::collections::HashSet::new();
    let mut count_fetch_handle: Option<tokio::task::JoinHandle<()>> = None;
    let mut log_stream_handle: Option<tokio::task::JoinHandle<()>> = None;

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

                        app.next_generation();
                        abort_all_watchers(&mut watcher_handles, &mut active_watch_types);
                        if let Some(h) = count_fetch_handle.take() {
                            h.abort();
                        }
                        if let Some(h) = log_stream_handle.take() {
                            h.abort();
                        }
                        app.loading = true;
                        app.resources_by_type.clear();
                        // Clear counts — they were for the previous context.
                        // A new count fetch will be started on ContextSwitchReady.
                        app.resource_counts.clear();

                        // Async context switch + namespace list; then signal main loop.
                        // On success, mark the cluster as reachable (clears unreachable status).
                        let ctx_for_probe = context_name.clone();
                        let handle = tokio::spawn(async move {
                            let mut guard = mgr.lock().await;
                            if let Some(ref mut manager) = *guard {
                                if let Err(e) =
                                    manager.switch_context(&context_name).await
                                {
                                    event::send_event(
                                        &action_tx,
                                        AppEvent::K8sError(format!(
                                            "Failed to switch context: {}",
                                            e
                                        )),
                                    );
                                    event::send_event(
                                        &action_tx,
                                        AppEvent::ClusterProbeResult {
                                            context: ctx_for_probe,
                                            reachable: false,
                                        },
                                    );
                                    return;
                                }
                                match manager.list_namespaces().await {
                                    Ok(namespaces) => {
                                        event::send_event(
                                            &action_tx,
                                            AppEvent::NamespacesLoaded(namespaces),
                                        );
                                        // Successfully connected — mark reachable
                                        event::send_event(
                                            &action_tx,
                                            AppEvent::ClusterProbeResult {
                                                context: ctx_for_probe,
                                                reachable: true,
                                            },
                                        );
                                    }
                                    Err(e) => {
                                        event::send_event(
                                            &action_tx,
                                            AppEvent::K8sError(format!(
                                                "Failed to list namespaces: {}",
                                                e
                                            )),
                                        );
                                        event::send_event(
                                            &action_tx,
                                            AppEvent::ClusterProbeResult {
                                                context: ctx_for_probe,
                                                reachable: false,
                                            },
                                        );
                                    }
                                }
                            }
                            // Signal main loop to start watchers
                            event::send_event(&action_tx, AppEvent::ContextSwitchReady);
                        });
                        watcher_handles.push(handle);
                    }
                    InputAction::NamespaceChanged => {
                        app.next_generation();
                        abort_all_watchers(&mut watcher_handles, &mut active_watch_types);
                        if let Some(h) = count_fetch_handle.take() {
                            h.abort();
                        }
                        if let Some(h) = log_stream_handle.take() {
                            h.abort();
                        }
                        app.loading = true;
                        app.resources_by_type.clear();
                        app.resource_counts.clear();
                        app.select_first_row();

                        let ns = app.current_namespace().to_string();
                        count_fetch_handle = Some(start_count_fetch(
                            &app,
                            &k8s_manager,
                            &tx,
                            &ns,
                        ));
                        start_watchers(&app, &k8s_manager, &tx, &mut watcher_handles, &mut active_watch_types);
                    }
                    InputAction::ResourceTypeChanged => {
                        // Only watchers (which are type-scoped) need to be
                        // aborted here. The count fetch is namespace-scoped
                        // and is deliberately left running so that its
                        // results can populate the type selector.
                        app.next_generation();
                        abort_all_watchers(&mut watcher_handles, &mut active_watch_types);
                        app.loading = true;
                        app.resources_by_type.clear();
                        app.select_first_row();

                        start_watchers(&app, &k8s_manager, &tx, &mut watcher_handles, &mut active_watch_types);
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
                                        event::send_event(&action_tx,AppEvent::K8sError(
                                            format!("Describe error: {}", e),
                                        ));
                                    }
                                }
                            }
                        });
                    }
                    InputAction::StreamLogs => {
                        // Cancel any existing log stream
                        if let Some(h) = log_stream_handle.take() {
                            h.abort();
                        }

                        // Use the pod's own namespace so the stream works
                        // when the "all namespaces" option is selected.
                        let (name, ns) = match app.selected_resource() {
                            Some((res, _)) => {
                                let ns = if res.namespace.is_empty() {
                                    app.current_namespace().to_string()
                                } else {
                                    res.namespace.clone()
                                };
                                (res.name.clone(), ns)
                            }
                            None => continue,
                        };
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();

                        app.loading = true;

                        log_stream_handle = Some(tokio::spawn(async move {
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
                                    event::send_event(
                                        &action_tx,
                                        AppEvent::K8sError(format!(
                                            "Log stream error: {}",
                                            e
                                        )),
                                    );
                                }
                            }
                        }));
                    }
                    InputAction::StopLogs => {
                        if let Some(h) = log_stream_handle.take() {
                            h.abort();
                        }
                    }
                    InputAction::Delete => {
                        let (name, ns, rt) = {
                            if let Some((res, rt)) = app.selected_resource() {
                                (res.name.clone(), res.namespace.clone(), rt)
                            } else {
                                continue;
                            }
                        };
                        // Fall back to the current namespace only when the
                        // resource has no namespace of its own (cluster-scoped
                        // types). For namespaced types this keeps Delete
                        // correct even when the "all namespaces" selector
                        // option is active.
                        let ns = if ns.is_empty() {
                            app.current_namespace().to_string()
                        } else {
                            ns
                        };
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
                                    event::send_event(&action_tx,AppEvent::K8sError(format!(
                                        "Delete error: {}",
                                        e
                                    )));
                                }
                            }
                        });
                    }
                    InputAction::Restart => {
                        let (name, ns, rt) = {
                            if let Some((res, rt)) = app.selected_resource() {
                                (res.name.clone(), res.namespace.clone(), rt)
                            } else {
                                continue;
                            }
                        };
                        // Use the resource's own namespace so Restart works
                        // correctly when the "all namespaces" option is
                        // selected.
                        let ns = if ns.is_empty() {
                            app.current_namespace().to_string()
                        } else {
                            ns
                        };
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
                                    event::send_event(&action_tx,AppEvent::K8sError(format!(
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

                            let _ = open_logs_in_editor(&app.log_lines);

                            // Editor's LeaveAlternateScreen put us on the main screen;
                            // re-enter alternate screen so kterm draws there, not on scrollback.
                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                            enable_raw_mode()?;
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
                                // Use the pod's own namespace so the live log
                                // tail works when "all namespaces" is the
                                // selected namespace option.
                                let (pod_ns, pod_name) = match app.selected_resource() {
                                    Some((res, _)) => {
                                        let ns = if res.namespace.is_empty() {
                                            app.current_namespace().to_string()
                                        } else {
                                            res.namespace.clone()
                                        };
                                        (ns, res.name.clone())
                                    }
                                    None => (
                                        app.current_namespace().to_string(),
                                        String::new(),
                                    ),
                                };
                                let guard = k8s_manager.lock().await;
                                guard
                                    .as_ref()
                                    .map(|mgr| (mgr.client.clone(), pod_ns, pod_name))
                            };

                            events.suspend();
                            disable_raw_mode()?;

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

                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                            enable_raw_mode()?;
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
                            // Use the resource's own namespace so edits apply
                            // to the correct namespace when "all namespaces"
                            // is selected.
                            let ns = if resource.namespace.is_empty() {
                                app.current_namespace().to_string()
                            } else {
                                resource.namespace.clone()
                            };
                            let mgr = k8s_manager.clone();
                            let action_tx = tx.clone();

                            events.suspend();
                            disable_raw_mode()?;

                            let edited = edit_yaml_in_editor(&yaml);

                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                            enable_raw_mode()?;
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
                                            event::send_event(&action_tx,AppEvent::K8sError(
                                                format!("Apply error: {}", e),
                                            ));
                                        }
                                    }
                                });
                            }
                        }
                    }
                    InputAction::Exec => {
                        if let Some((resource, rt)) = app.selected_resource() {
                            if rt.supports_exec() {
                                let name = resource.name.clone();
                                let ns = if resource.namespace.is_empty() {
                                    app.current_namespace().to_string()
                                } else {
                                    resource.namespace.clone()
                                };

                                let client = {
                                    let guard = k8s_manager.lock().await;
                                    guard.as_ref().map(|m| m.client.clone())
                                };

                                if let Some(client) = client {
                                    events.suspend();
                                    // Stay in raw mode so keystrokes flow as
                                    // bytes into the websocket pump. Clear
                                    // the alternate screen and show the
                                    // cursor so the shell prompt lands at
                                    // the top of a blank window.
                                    execute!(
                                        terminal.backend_mut(),
                                        Clear(ClearType::All),
                                        crossterm::cursor::MoveTo(0, 0),
                                        crossterm::cursor::Show,
                                    )?;

                                    let exec_result =
                                        exec_into_pod(client, &ns, &name).await;

                                    execute!(
                                        terminal.backend_mut(),
                                        crossterm::cursor::Hide,
                                    )?;
                                    terminal.clear()?;
                                    events.resume();

                                    if let Err(e) = exec_result {
                                        app.set_error(format!("Exec error: {}", e));
                                    }
                                } else {
                                    app.set_error(
                                        "No Kubernetes client available".to_string(),
                                    );
                                }
                            }
                        }
                    }
                    InputAction::StartSearch => {
                        let contexts = app.contexts.clone();
                        let unreachable = app.unreachable_contexts.clone();

                        // Filter out unreachable clusters from search
                        let reachable_contexts: Vec<String> = contexts
                            .into_iter()
                            .filter(|c| !unreachable.contains(c))
                            .collect();

                        if reachable_contexts.is_empty() {
                            app.set_error(
                                "All clusters are unreachable. Select a cluster to retry connecting.".to_string(),
                            );
                            app.search_loading = false;
                        } else {
                            app.search_contexts_total = reachable_contexts.len();
                            app.search_contexts_done = 0;

                            for context in reachable_contexts {
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
                                                        event::send_event(&search_tx,
                                                            AppEvent::SearchResultsBatch {
                                                                context: ctx.clone(),
                                                                resource_type: rt,
                                                                items,
                                                            },
                                                        );
                                                    }
                                                    Err(_) => {
                                                        // Silently skip individual resource type
                                                        // errors (could be RBAC, not connectivity).
                                                    }
                                                }
                                            }
                                        }
                                        Err(_) => {
                                            // Cluster became unreachable — mark it and show one error.
                                            event::send_event(&search_tx,
                                                AppEvent::ClusterProbeResult {
                                                    context: ctx.clone(),
                                                    reachable: false,
                                                },
                                            );
                                            event::send_event(&search_tx,AppEvent::K8sError(
                                                format!("Cluster {} is now unreachable", ctx),
                                            ));
                                        }
                                    }
                                    let _ = search_tx
                                        .send(AppEvent::SearchScanComplete(ctx));
                                });
                            }
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
                                                event::send_event(&action_tx,
                                                    AppEvent::K8sError(format!(
                                                        "Describe error: {}",
                                                        e
                                                    )),
                                                );
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        event::send_event(&action_tx,AppEvent::K8sError(
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
                            // Cancel any existing log stream
                            if let Some(h) = log_stream_handle.take() {
                                h.abort();
                            }

                            let action_tx = tx.clone();
                            app.loading = true;

                            log_stream_handle = Some(tokio::spawn(async move {
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
                                            event::send_event(&action_tx,AppEvent::K8sError(
                                                format!("Log stream error: {}", e),
                                            ));
                                        }
                                    }
                                    Err(e) => {
                                        event::send_event(&action_tx,AppEvent::K8sError(
                                            format!(
                                                "Connect to {}: {}",
                                                result.context, e
                                            ),
                                        ));
                                    }
                                }
                            }));
                        }
                    }
                    InputAction::None => {}
                }
            }
            AppEvent::Tick => {
                app.handle_tick();
            }
            AppEvent::Resize(_, _) => {}
            AppEvent::ResourcesUpdatedForType {
                resource_type: rt,
                items,
                generation,
            } => {
                // Discard stale events from a previous generation
                if generation != app.generation {
                    continue;
                }
                app.resources_by_type.insert(rt, items);
                app.loading = false;
                let rows = app.display_rows();
                let len = rows.len();
                if len > 0 {
                    match app.table_state.selected() {
                        Some(selected) if selected >= len => {
                            app.table_state.select(Some(len - 1));
                        }
                        None => {
                            app.select_first_row();
                        }
                        _ => {}
                    }
                }
            }
            AppEvent::NamespacesLoaded(namespaces) => {
                // Prepend the "all namespaces" sentinel so it's always the
                // first entry and can be selected from the dropdown.
                let mut with_all = Vec::with_capacity(namespaces.len() + 1);
                with_all.push(types::ALL_NAMESPACES_LABEL.to_string());
                with_all.extend(namespaces);
                app.namespaces = with_all;

                // Default to the first real namespace (index 1), falling back
                // to the sentinel (index 0) if the cluster returned none.
                let default_idx = if app.namespaces.len() > 1 { 1 } else { 0 };
                if let Some(ref pref) = app.preferred_namespace {
                    if let Some(idx) = app.namespaces.iter().position(|n| n == pref) {
                        app.selected_namespaces.clear();
                        app.selected_namespaces.insert(idx);
                    } else {
                        app.selected_namespaces.clear();
                        app.selected_namespaces.insert(default_idx);
                    }
                } else {
                    app.selected_namespaces.clear();
                    app.selected_namespaces.insert(default_idx);
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
                current_reachable,
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

                // Mark current context unreachable if it failed namespace listing
                if !current_reachable {
                    app.unreachable_contexts.insert(current.clone());
                }

                // Probe all other clusters for connectivity in the background.
                // If the current cluster is unreachable, it's already marked above
                // (no need to re-probe since we just tried).
                let probe_count = app.contexts.iter().filter(|c| c.as_str() != current).count();
                app.cluster_probes_pending = probe_count;

                // If this is the only context and it's unreachable, show error now
                if probe_count == 0 && !current_reachable {
                    app.set_error(
                        "All clusters are unreachable. Select a cluster to retry connecting."
                            .to_string(),
                    );
                }

                for ctx_name in &app.contexts {
                    if ctx_name == &current {
                        continue;
                    }
                    let probe_ctx = ctx_name.clone();
                    let probe_tx = tx.clone();
                    tokio::spawn(async move {
                        let reachable = match k8s::client::K8sManager::client_for_context(&probe_ctx).await {
                            Ok(client) => {
                                // Use /version endpoint: no RBAC needed, no etcd hit, tiny response.
                                tokio::time::timeout(
                                    k8s::K8S_TIMEOUT,
                                    client.apiserver_version(),
                                )
                                .await
                                .is_ok_and(|r| r.is_ok())
                            }
                            Err(_) => false,
                        };
                        event::send_event(
                            &probe_tx,
                            AppEvent::ClusterProbeResult {
                                context: probe_ctx,
                                reachable,
                            },
                        );
                    });
                }

                // Start initial resource watchers and count fetch.
                abort_all_watchers(&mut watcher_handles, &mut active_watch_types);
                if let Some(h) = count_fetch_handle.take() {
                    h.abort();
                }
                let ns = app.current_namespace().to_string();
                count_fetch_handle = Some(start_count_fetch(&app, &k8s_manager, &tx, &ns));
                start_watchers(&app, &k8s_manager, &tx, &mut watcher_handles, &mut active_watch_types);
            }
            AppEvent::ResourceCountsLoaded { counts, context, namespace } => {
                // Discard counts fetched for a different context/namespace.
                // Counts are namespace-scoped, so type changes do NOT
                // invalidate them — only context or namespace changes do.
                if context != app.current_context() || namespace != app.current_namespace() {
                    continue;
                }
                app.resource_counts = counts;
                if let types::Focus::Selector(types::SelectorTarget::ResourceType) = app.focus
                {
                    app.update_dropdown_filter();
                }
            }
            AppEvent::ContextSwitchReady => {
                // Context switch async work is done; start watchers and
                // count fetch from main loop so handles are tracked.
                if let Some(h) = count_fetch_handle.take() {
                    h.abort();
                }
                let ns = app.current_namespace().to_string();
                count_fetch_handle = Some(start_count_fetch(&app, &k8s_manager, &tx, &ns));
                start_watchers(&app, &k8s_manager, &tx, &mut watcher_handles, &mut active_watch_types);
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
            AppEvent::ClusterProbeResult { context, reachable } => {
                if reachable {
                    app.unreachable_contexts.remove(&context);
                } else {
                    app.unreachable_contexts.insert(context);
                }
                // Track startup probe completion
                if app.cluster_probes_pending > 0 {
                    app.cluster_probes_pending -= 1;
                    if app.cluster_probes_pending == 0
                        && app.unreachable_contexts.len() >= app.contexts.len()
                    {
                        app.set_error(
                            "All clusters are unreachable. Select a cluster to retry connecting."
                                .to_string(),
                        );
                    }
                }
                // Refresh the dropdown if the cluster selector is open
                if let types::Focus::Selector(types::SelectorTarget::Context) = app.focus {
                    app.update_dropdown_filter();
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

/// Strip fields that are not useful for editing (like kubectl edit does).
fn strip_managed_fields(yaml: &str) -> String {
    if let Ok(mut value) = serde_yaml::from_str::<serde_yaml::Value>(yaml) {
        if let Some(metadata) = value.get_mut("metadata").and_then(|m| m.as_mapping_mut()) {
            metadata.remove(serde_yaml::Value::String("managedFields".to_string()));
        }
        serde_yaml::to_string(&value).unwrap_or_else(|_| yaml.to_string())
    } else {
        yaml.to_string()
    }
}

/// Open an interactive shell session inside a pod over kube-rs's existing
/// authenticated websocket connection.
///
/// Faster than shelling out to `kubectl` because it skips re-parsing
/// kubeconfig, re-running auth-exec plugins (gke-gcloud-auth-plugin,
/// aws-iam-authenticator, ...), and a fresh TLS handshake. The terminal
/// must already be in raw mode and in the alternate screen; the caller
/// is responsible for clearing it before this is invoked.
async fn exec_into_pod(
    client: kube::Client,
    namespace: &str,
    pod: &str,
) -> Result<()> {
    use futures::SinkExt;
    use k8s_openapi::api::core::v1::Pod;
    use kube::api::{AttachParams, TerminalSize};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Forward the local TERM into the container so colors / readline /
    // termcap-driven apps (vim, htop, less) behave the same as if the
    // user had ssh'd in.
    let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());
    let script = format!(
        "TERM={term}; export TERM; \
         if   command -v fish >/dev/null 2>&1; then exec fish; \
         elif command -v bash >/dev/null 2>&1; then exec bash; \
         else exec sh; \
         fi"
    );

    let api: kube::Api<Pod> = kube::Api::namespaced(client, namespace);
    let mut attached = api
        .exec(
            pod,
            vec!["sh".to_string(), "-c".to_string(), script],
            &AttachParams::interactive_tty(),
        )
        .await
        .context("Failed to start exec session")?;

    let mut stdin_pipe = attached
        .stdin()
        .ok_or_else(|| anyhow::anyhow!("exec session has no stdin"))?;
    let mut stdout_pipe = attached
        .stdout()
        .ok_or_else(|| anyhow::anyhow!("exec session has no stdout"))?;
    let resize_sender = attached.terminal_size();

    // Pump local stdin → websocket. Local terminal is already in raw mode
    // so each keystroke arrives as its raw byte sequence.
    let stdin_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        let mut tokio_stdin = tokio::io::stdin();
        loop {
            match tokio_stdin.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if stdin_pipe.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    let _ = stdin_pipe.flush().await;
                }
            }
        }
    });

    // Pump websocket → local stdout.
    let stdout_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        let mut tokio_stdout = tokio::io::stdout();
        loop {
            match stdout_pipe.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tokio_stdout.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    let _ = tokio_stdout.flush().await;
                }
            }
        }
    });

    // Send the initial terminal size and forward future SIGWINCH events
    // so the remote PTY tracks the local window dimensions.
    let resize_task = if let Some(mut sender) = resize_sender {
        if let Ok((cols, rows)) = crossterm::terminal::size() {
            let _ = sender
                .send(TerminalSize {
                    width: cols,
                    height: rows,
                })
                .await;
        }
        Some(tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut winch = match signal(SignalKind::window_change()) {
                Ok(s) => s,
                Err(_) => return,
            };
            while winch.recv().await.is_some() {
                if let Ok((cols, rows)) = crossterm::terminal::size() {
                    if sender
                        .send(TerminalSize {
                            width: cols,
                            height: rows,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }))
    } else {
        None
    };

    // Wait for the remote shell to exit (websocket closed by either side).
    let _ = attached.join().await;

    stdin_task.abort();
    stdout_task.abort();
    if let Some(t) = resize_task {
        t.abort();
    }

    Ok(())
}

fn edit_yaml_in_editor(yaml: &str) -> Result<Option<String>> {
    use std::io::Write;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    let cleaned_yaml = strip_managed_fields(yaml);

    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(cleaned_yaml.as_bytes())?;
    tmp.flush()?;

    let path = tmp.path().to_owned();
    let status = std::process::Command::new(&editor).arg(&path).status()?;

    if !status.success() {
        return Ok(None);
    }

    let new_content = std::fs::read_to_string(&path)?;
    if new_content == cleaned_yaml {
        return Ok(None);
    }

    Ok(Some(new_content))
}
