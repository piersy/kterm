use crossterm::event::{EventStream, KeyEvent};
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::types::{ResourceItem, ResourceType};

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
    ResourcesUpdated(Vec<ResourceItem>),
    NamespacesLoaded(Vec<String>),
    DetailLoaded(String),
    LogLine(String),
    LogStreamEnded,
    ContextsLoaded {
        contexts: Vec<String>,
        current: String,
        current_namespace: String,
    },
    K8sError(String),
    SearchResultsBatch {
        context: String,
        resource_type: ResourceType,
        items: Vec<ResourceItem>,
    },
    SearchScanComplete(String),
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    tx: mpsc::UnboundedSender<AppEvent>,
    crossterm_task: Option<tokio::task::JoinHandle<()>>,
    _tick_task: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let crossterm_task = Self::spawn_crossterm_reader(tx.clone());

        let tick_tx = tx.clone();
        let tick_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
            loop {
                interval.tick().await;
                if tick_tx.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        Self {
            rx,
            tx,
            crossterm_task: Some(crossterm_task),
            _tick_task: tick_task,
        }
    }

    fn spawn_crossterm_reader(
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut reader = EventStream::new();
            loop {
                match reader.next().await {
                    Some(Ok(crossterm::event::Event::Key(key))) => {
                        if tx.send(AppEvent::Key(key)).is_err() {
                            break;
                        }
                    }
                    Some(Ok(crossterm::event::Event::Resize(w, h))) => {
                        if tx.send(AppEvent::Resize(w, h)).is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        })
    }

    fn drain_stale_input_events(&mut self) {
        let mut kept = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            match event {
                AppEvent::Key(_) | AppEvent::Resize(_, _) => {
                    // Discard stale terminal input events
                }
                other => kept.push(other),
            }
        }
        for event in kept {
            let _ = self.tx.send(event);
        }
    }

    /// Suspend the crossterm reader task and drain any stale key/resize
    /// events from the channel. Call this before launching a subprocess
    /// that needs stdin.
    pub fn suspend(&mut self) {
        if let Some(task) = self.crossterm_task.take() {
            task.abort();
        }
        self.drain_stale_input_events();
    }

    /// Resume the crossterm reader task. Call this after a subprocess
    /// has exited and the terminal has been restored.
    pub fn resume(&mut self) {
        self.drain_stale_input_events();
        self.crossterm_task = Some(Self::spawn_crossterm_reader(self.tx.clone()));
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    /// Non-blocking receive. Returns Ok(event) if an event is available,
    /// or Err if the channel is empty.
    pub fn try_recv(&mut self) -> Result<AppEvent, mpsc::error::TryRecvError> {
        self.rx.try_recv()
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.tx.clone()
    }
}
