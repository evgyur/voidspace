use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

const MAX_LOG_BYTES: u64 = 1024 * 1024;

pub fn redact(value: &str) -> String {
    let mut result = value.replace('\r', "\\r").replace('\n', "\\n");
    if let Ok(profile) = std::env::var("USERPROFILE")
        && !profile.is_empty()
    {
        result = result.replace(&profile, "%USERPROFILE%");
    }
    result
}

pub fn log_line(value: &str) -> std::io::Result<()> {
    let path = diagnostics_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if path
        .metadata()
        .is_ok_and(|metadata| metadata.len() >= MAX_LOG_BYTES)
    {
        let rotated = path.with_extension("log.1");
        let _ = fs::remove_file(&rotated);
        fs::rename(&path, rotated)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", redact(value))
}

pub fn diagnostics_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Voidspace")
        .join("diagnostics.log")
}

#[derive(Default)]
pub struct UiFrameDiagnostic {
    idle_since: Option<Instant>,
    settled_frames: VecDeque<Instant>,
}

impl UiFrameDiagnostic {
    pub fn record(&mut self, now: Instant, idle: bool) -> Option<usize> {
        if !idle {
            self.idle_since = None;
            self.settled_frames.clear();
            return None;
        }
        let idle_since = *self.idle_since.get_or_insert(now);
        if now.duration_since(idle_since) < Duration::from_secs(1) {
            return None;
        }
        self.settled_frames.push_back(now);
        while self
            .settled_frames
            .front()
            .is_some_and(|frame| now.duration_since(*frame) > Duration::from_secs(5))
        {
            self.settled_frames.pop_front();
        }
        (now.duration_since(idle_since) >= Duration::from_secs(6))
            .then_some(self.settled_frames.len())
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn idle_counter_excludes_settling_window_and_resets_on_activity() {
        let start = Instant::now();
        let mut counter = UiFrameDiagnostic::default();
        assert_eq!(counter.record(start, true), None);
        assert_eq!(counter.record(start + Duration::from_secs(1), true), None);
        assert_eq!(
            counter.record(start + Duration::from_secs(6), true),
            Some(2)
        );
        assert_eq!(counter.record(start + Duration::from_secs(7), false), None);
        assert_eq!(counter.record(start + Duration::from_secs(14), true), None);
    }
}
