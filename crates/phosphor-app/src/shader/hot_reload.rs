use std::path::{Path, PathBuf};

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind, Debouncer};

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
        let shader_dir = Path::new("assets/shaders");
        if shader_dir.exists() {
            debouncer
                .watcher()
                .watch(shader_dir, notify::RecursiveMode::Recursive)?;
            log::info!("Watching {} for shader changes", shader_dir.display());
        }

        Ok(Self {
            _debouncer: debouncer,
            receiver: rx,
        })
    }

    pub fn watch_path(&mut self, path: &Path) -> Result<()> {
        self._debouncer
            .watcher()
            .watch(path, notify::RecursiveMode::Recursive)?;
        log::info!("Watching {} for shader changes", path.display());
        Ok(())
    }

    /// Drain all pending changes and return the most recent one.
    pub fn check_for_changes(&self) -> Option<PathBuf> {
        let mut last = None;
        while let Ok(path) = self.receiver.try_recv() {
            last = Some(path);
        }
        last
    }
}
