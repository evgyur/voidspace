use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
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
