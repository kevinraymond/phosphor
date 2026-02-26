use std::path::PathBuf;

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind, Debouncer};

use crate::effect::loader::assets_dir;

pub struct ShaderWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher>,
    receiver: Receiver<PathBuf>,
}

impl ShaderWatcher {
    pub fn new() -> Result<Self> {
        let (tx, rx): (Sender<PathBuf>, Receiver<PathBuf>) = crossbeam_channel::unbounded();

        let mut debouncer = new_debouncer(
            std::time::Duration::from_millis(100),
            move |res: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                if let Ok(events) = res {
                    for event in events {
                        if event.kind == DebouncedEventKind::Any {
                            let path = event.path.clone();
                            if path.extension().is_some_and(|ext| ext == "wgsl") {
                                let _ = tx.send(path);
                            }
                        }
                    }
                }
            },
        )?;

        // Watch assets/shaders by default
        let shader_dir = assets_dir().join("shaders");
        if shader_dir.exists() {
            debouncer
                .watcher()
                .watch(&shader_dir, notify::RecursiveMode::Recursive)?;
            log::info!("Watching {} for shader changes", shader_dir.display());
        }

        Ok(Self {
            _debouncer: debouncer,
            receiver: rx,
        })
    }

    /// Drain all pending change events and return the unique paths.
    pub fn drain_changes(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        while let Ok(path) = self.receiver.try_recv() {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths
    }
}
