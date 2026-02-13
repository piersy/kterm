use crossterm::event::{EventStream, KeyEvent};
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::types::ResourceItem;

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
    },
    K8sError(String),
}

pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
    tx: mpsc::UnboundedSender<AppEvent>,
    _crossterm_task: tokio::task::JoinHandle<()>,
    _tick_task: tokio::task::JoinHandle<()>,
}

impl EventHandler {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let crossterm_tx = tx.clone();
        let crossterm_task = tokio::spawn(async move {
            let mut reader = EventStream::new();
            loop {
                match reader.next().await {
                    Some(Ok(crossterm::event::Event::Key(key))) => {
                        if crossterm_tx.send(AppEvent::Key(key)).is_err() {
                            break;
                        }
                    }
                    Some(Ok(crossterm::event::Event::Resize(w, h))) => {
                        if crossterm_tx.send(AppEvent::Resize(w, h)).is_err() {
                            break;
                        }
                    }
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        });

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
            _crossterm_task: crossterm_task,
            _tick_task: tick_task,
        }
    }

    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.tx.clone()
    }
}
