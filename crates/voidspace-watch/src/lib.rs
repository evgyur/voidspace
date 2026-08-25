//! Continuous filesystem observation and bounded invalidation.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crossbeam_channel::{Sender, TrySendError};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct WatchRequest {
    pub root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WatchSignal {
    Changed { sequence: u64, paths: Vec<PathBuf> },
    Invalidated { sequence: u64, reason: String },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WatchHealth {
    pub sequence: u64,
    pub overflowed: bool,
}

#[derive(Debug, Error)]
pub enum WatchError {
    #[error("watcher error: {0}")]
    Notify(#[from] notify::Error),
}

pub struct WatchHandle {
    _watcher: RecommendedWatcher,
    sequence: Arc<AtomicU64>,
    overflowed: Arc<AtomicBool>,
}

impl WatchHandle {
    pub fn health(&self) -> WatchHealth {
        WatchHealth {
            sequence: self.sequence.load(Ordering::Acquire),
            overflowed: self.overflowed.load(Ordering::Acquire),
        }
    }

    pub fn stop(self) {}
}

pub fn watch(request: WatchRequest, sink: Sender<WatchSignal>) -> Result<WatchHandle, WatchError> {
    let sequence = Arc::new(AtomicU64::new(0));
    let overflowed = Arc::new(AtomicBool::new(false));
    let callback_sequence = Arc::clone(&sequence);
    let callback_overflowed = Arc::clone(&overflowed);
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<Event>| {
            let sequence = callback_sequence.fetch_add(1, Ordering::AcqRel) + 1;
            let signal = match result {
                Ok(event) => WatchSignal::Changed {
                    sequence,
                    paths: event.paths,
                },
                Err(error) => WatchSignal::Invalidated {
                    sequence,
                    reason: error.to_string(),
                },
            };
            match sink.try_send(signal) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    callback_overflowed.store(true, Ordering::Release);
                }
                Err(TrySendError::Disconnected(_)) => {}
            }
        },
        Config::default(),
    )?;
    watcher.watch(&request.root, RecursiveMode::Recursive)?;
    Ok(WatchHandle {
        _watcher: watcher,
        sequence,
        overflowed,
    })
}

pub fn common_ancestor(root: &std::path::Path, paths: &[PathBuf]) -> PathBuf {
    let Some(first) = paths.first() else {
        return root.to_path_buf();
    };
    let mut ancestor = first.clone();
    while !paths.iter().all(|path| path.starts_with(&ancestor)) {
        if !ancestor.pop() {
            return root.to_path_buf();
        }
    }
    if ancestor.starts_with(root) {
        ancestor
    } else {
        root.to_path_buf()
    }
}
