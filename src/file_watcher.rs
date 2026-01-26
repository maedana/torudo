use notify::{Watcher, RecursiveMode, RecommendedWatcher, Event as NotifyEvent};
use std::sync::mpsc;
use std::error::Error;
use std::path::Path;

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    rx: mpsc::Receiver<NotifyEvent>,
}

impl FileWatcher {
    pub fn new(_todotxt_dir: &str) -> Result<Self, Box<dyn Error>> {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res: Result<NotifyEvent, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            notify::Config::default(),
        )?;
        
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    pub fn start_watching(&mut self, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
        self._watcher.watch(Path::new(todotxt_dir), RecursiveMode::NonRecursive)?;
        Ok(())
    }

    pub fn receiver(&self) -> &mpsc::Receiver<NotifyEvent> {
        &self.rx
    }
}