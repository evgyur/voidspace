use std::{char::decode_utf16, fmt};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A lossless Windows filename represented as exact UTF-16 code units.
#[derive(Clone, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct WinName(Vec<u16>);

#[derive(Debug, Error, Eq, PartialEq)]
pub enum WinNameError {
    #[error("Windows names cannot contain NUL code units")]
    EmbeddedNul,
    #[error("Windows name exceeds 32,767 UTF-16 code units")]
    TooLong,
}

impl WinName {
    pub const MAX_CODE_UNITS: usize = 32_767;

    pub fn from_units(units: Vec<u16>) -> Result<Self, WinNameError> {
        if units.len() > Self::MAX_CODE_UNITS {
            return Err(WinNameError::TooLong);
        }
        if units.contains(&0) {
            return Err(WinNameError::EmbeddedNul);
        }
        Ok(Self(units))
    }

    pub fn units(&self) -> &[u16] {
        &self.0
    }

    pub fn to_utf16le_bytes(&self) -> Vec<u8> {
        self.0.iter().flat_map(|unit| unit.to_le_bytes()).collect()
    }

    pub fn display_escaped(&self) -> String {
        decode_utf16(self.0.iter().copied())
            .map(|decoded| match decoded {
                Ok(ch) => ch.to_string(),
                Err(err) => format!("\\u{:04X}", err.unpaired_surrogate()),
            })
            .collect()
    }

    #[cfg(windows)]
    pub fn from_os_str(value: &std::ffi::OsStr) -> Result<Self, WinNameError> {
        use std::os::windows::ffi::OsStrExt;
        Self::from_units(value.encode_wide().collect())
    }

    #[cfg(windows)]
    pub fn to_os_string(&self) -> std::ffi::OsString {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&self.0)
    }
}

impl From<&str> for WinName {
    fn from(value: &str) -> Self {
        Self(value.encode_utf16().collect())
    }
}

impl From<String> for WinName {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl fmt::Debug for WinName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("WinName")
            .field(&self.display_escaped())
            .finish()
    }
}

impl fmt::Display for WinName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_escaped())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_unpaired_surrogate_without_loss() {
        let name = WinName::from_units(vec![b'a' as u16, 0xD800]).unwrap();
        assert_eq!(name.display_escaped(), "a\\uD800");
        assert_eq!(name.units(), &[0x0061, 0xD800]);
    }
}
