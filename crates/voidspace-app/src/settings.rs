use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u16,
    pub last_scope: String,
    pub always_request_admin: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            last_scope: std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into()),
            always_request_admin: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        Self::load_from(&path).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), voidspace_export::ExportError> {
        self.save_to(&settings_path())
    }

    pub fn load_from(path: &Path) -> Result<Self, voidspace_export::ExportError> {
        let settings: Self = voidspace_export::read_json(path)?;
        if settings.version != 1 {
            return Ok(Self::default());
        }
        Ok(settings)
    }

    pub fn save_to(&self, path: &Path) -> Result<(), voidspace_export::ExportError> {
        voidspace_export::write_json_atomic(path, self)
    }
}

pub fn settings_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Voidspace")
        .join("settings.json")
}
