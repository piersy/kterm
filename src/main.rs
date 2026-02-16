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

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let mut app = App::new();
    let mut events = EventHandler::new();
    let tx = events.sender();

    // Try to connect to Kubernetes
    app.loading = true;
    let k8s_tx = tx.clone();
    tokio::spawn(async move {
        match k8s::client::K8sManager::new().await {
            Ok(manager) => {
                let contexts = manager.context_names();
                let current = manager.current_context.clone();

                // Load namespaces
                match manager.list_namespaces().await {
                    Ok(namespaces) => {
                        let _ = k8s_tx.send(AppEvent::NamespacesLoaded(namespaces));
                    }
                    Err(e) => {
                        let _ = k8s_tx.send(AppEvent::K8sError(format!(
                            "Failed to list namespaces: {}",
                            e
                        )));
                        let _ = k8s_tx.send(AppEvent::NamespacesLoaded(vec!["default".to_string()]));
                    }
                }

                // Start watching resources
                let ns = "default".to_string();
                let watch_tx = k8s_tx.clone();
                let watch_client = manager.client.clone();
                tokio::spawn(async move {
                    if let Err(e) = k8s::resources::watch_resources(
                        watch_client,
                        &ns,
                        crate::types::ResourceType::Pods,
                        watch_tx.clone(),
                    )
                    .await
                    {
                        let _ = watch_tx.send(AppEvent::K8sError(format!("Watch error: {}", e)));
                    }
                });

                let _ = k8s_tx.send(AppEvent::ContextsLoaded { contexts, current });
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

    // Shared K8s manager for actions (wrapped in Arc<Mutex>)
    let k8s_manager: std::sync::Arc<tokio::sync::Mutex<Option<k8s::client::K8sManager>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(None));

    // Try to init the manager for actions
    {
        let mgr = k8s_manager.clone();
        tokio::spawn(async move {
            if let Ok(manager) = k8s::client::K8sManager::new().await {
                *mgr.lock().await = Some(manager);
            }
        });
    }

    // Track the current watcher task so we can abort it
    let mut watcher_handle: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        terminal.draw(|f| ui::render(f, &mut app))?;

        let Some(event) = events.next().await else {
            break;
        };

        match event {
            AppEvent::Key(key) => {
                // Only handle key press events (not release/repeat)
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                let action = app.handle_input(key);

                match action {
                    InputAction::ContextChanged => {
                        let context_name = app.current_context().to_string();
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();
                        let ns = app.current_namespace().to_string();
                        let rt = app.resource_type;

                        // Abort current watcher
                        if let Some(h) = watcher_handle.take() {
                            h.abort();
                        }

                        app.loading = true;
                        app.resources.clear();

                        tokio::spawn(async move {
                            let mut guard = mgr.lock().await;
                            if let Some(ref mut manager) = *guard {
                                if let Err(e) = manager.switch_context(&context_name).await {
                                    let _ = action_tx.send(AppEvent::K8sError(format!(
                                        "Failed to switch context: {}",
                                        e
                                    )));
                                    return;
                                }
                                // Reload namespaces
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
                                // Restart watcher
                                let client = manager.client.clone();
                                let watch_tx = action_tx.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = k8s::resources::watch_resources(
                                        client,
                                        &ns,
                                        rt,
                                        watch_tx.clone(),
                                    )
                                    .await
                                    {
                                        let _ = watch_tx.send(AppEvent::K8sError(format!(
                                            "Watch error: {}",
                                            e
                                        )));
                                    }
                                });
                            }
                        });
                    }
                    InputAction::NamespaceChanged | InputAction::ResourceTypeChanged => {
                        // Abort current watcher and start new one
                        if let Some(h) = watcher_handle.take() {
                            h.abort();
                        }

                        app.loading = true;
                        app.resources.clear();
                        app.table_state.select(Some(0));

                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();
                        let ns = app.current_namespace().to_string();
                        let rt = app.resource_type;

                        let handle = tokio::spawn(async move {
                            let guard = mgr.lock().await;
                            if let Some(ref manager) = *guard {
                                let client = manager.client.clone();
                                drop(guard); // release lock before long operation
                                if let Err(e) = k8s::resources::watch_resources(
                                    client,
                                    &ns,
                                    rt,
                                    action_tx.clone(),
                                )
                                .await
                                {
                                    let _ = action_tx.send(AppEvent::K8sError(format!(
                                        "Watch error: {}",
                                        e
                                    )));
                                }
                            }
                        });
                        watcher_handle = Some(handle);
                    }
                    InputAction::Describe => {
                        let name = app.selected_resource_name().unwrap_or_default();
                        let ns = app.current_namespace().to_string();
                        let rt = app.resource_type;
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();

                        app.loading = true;
                        app.detail_text.clear();

                        tokio::spawn(async move {
                            let guard = mgr.lock().await;
                            if let Some(ref manager) = *guard {
                                let client = manager.client.clone();
                                drop(guard);
                                match k8s::resources::describe_resource(client, &ns, &name, rt)
                                    .await
                                {
                                    Ok(desc) => {
                                        let _ = action_tx.send(AppEvent::DetailLoaded(desc));
                                    }
                                    Err(e) => {
                                        let _ = action_tx.send(AppEvent::K8sError(format!(
                                            "Describe error: {}",
                                            e
                                        )));
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
                    InputAction::StopLogs => {
                        // Log streaming will stop when the sender is dropped
                    }
                    InputAction::Delete => {
                        let name = app.selected_resource_name().unwrap_or_default();
                        let ns = app.current_namespace().to_string();
                        let rt = app.resource_type;
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();

                        tokio::spawn(async move {
                            let guard = mgr.lock().await;
                            if let Some(ref manager) = *guard {
                                let client = manager.client.clone();
                                drop(guard);
                                if let Err(e) =
                                    k8s::actions::delete_resource(client, &ns, &name, rt).await
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
                        let name = app.selected_resource_name().unwrap_or_default();
                        let ns = app.current_namespace().to_string();
                        let rt = app.resource_type;
                        let mgr = k8s_manager.clone();
                        let action_tx = tx.clone();

                        tokio::spawn(async move {
                            let guard = mgr.lock().await;
                            if let Some(ref manager) = *guard {
                                let client = manager.client.clone();
                                drop(guard);
                                if let Err(e) =
                                    k8s::actions::restart_resource(client, &ns, &name, rt).await
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
                            events.suspend();
                            disable_raw_mode()?;
                            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                            let _ = open_logs_in_less(&app.log_lines);

                            enable_raw_mode()?;
                            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                            terminal.clear()?;
                            events.resume();
                        }
                    }
                    InputAction::Edit => {
                        if let Some(resource) = app.selected_resource() {
                            let yaml = resource.raw_yaml.clone();
                            let name = resource.name.clone();
                            let ns = app.current_namespace().to_string();
                            let rt = app.resource_type;
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
                                            let _ = action_tx.send(AppEvent::K8sError(format!(
                                                "Apply error: {}",
                                                e
                                            )));
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
                                match k8s::client::K8sManager::client_for_context(&ctx).await {
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
                                                    let _ = search_tx.send(AppEvent::K8sError(
                                                        format!(
                                                            "Search {}/{}: {}",
                                                            ctx, rt, e
                                                        ),
                                                    ));
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = search_tx.send(AppEvent::K8sError(format!(
                                            "Connect to {}: {}",
                                            ctx, e
                                        )));
                                    }
                                }
                                let _ =
                                    search_tx.send(AppEvent::SearchScanComplete(ctx));
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
                                                let _ =
                                                    action_tx.send(AppEvent::K8sError(format!(
                                                        "Describe error: {}",
                                                        e
                                                    )));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = action_tx.send(AppEvent::K8sError(format!(
                                            "Connect to {}: {}",
                                            result.context, e
                                        )));
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
                                            let _ =
                                                action_tx.send(AppEvent::K8sError(format!(
                                                    "Log stream error: {}",
                                                    e
                                                )));
                                        }
                                    }
                                    Err(e) => {
                                        let _ = action_tx.send(AppEvent::K8sError(format!(
                                            "Connect to {}: {}",
                                            result.context, e
                                        )));
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
            AppEvent::Resize(_, _) => {
                // Terminal will re-draw on next loop
            }
            AppEvent::ResourcesUpdated(items) => {
                app.resources = items;
                app.loading = false;
                // Ensure selection stays in bounds
                let len = app.filtered_resources().len();
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
                app.selected_namespace = 0;
                app.loading = false;
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
            AppEvent::ContextsLoaded { contexts, current } => {
                app.contexts = contexts;
                if let Some(idx) = app.contexts.iter().position(|c| c == &current) {
                    app.selected_context = idx;
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

fn open_logs_in_less(log_lines: &[String]) -> Result<()> {
    let path = write_logs_to_tempfile(log_lines)?;

    std::process::Command::new("less")
        .arg("+F")
        .arg(&path)
        .status()?;

    let _ = std::fs::remove_file(&path);
    Ok(())
}

fn edit_yaml_in_editor(yaml: &str) -> Result<Option<String>> {
    use std::io::Write;

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let mut tmp = tempfile::NamedTempFile::new()?;
    tmp.write_all(yaml.as_bytes())?;
    tmp.flush()?;

    let path = tmp.path().to_owned();
    let status = std::process::Command::new(&editor)
        .arg(&path)
        .status()?;

    if !status.success() {
        return Ok(None);
    }

    let new_content = std::fs::read_to_string(&path)?;
    if new_content == yaml {
        return Ok(None); // No changes
    }

    Ok(Some(new_content))
}
