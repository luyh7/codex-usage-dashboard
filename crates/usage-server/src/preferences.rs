use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub schema_version: u32,
    pub language: Option<String>,
    pub timezone: Option<String>,
    pub codex_homes: Vec<PathBuf>,
}
pub fn load() -> Result<Preferences, String> {
    let path = crate::config::config_dir().join("preferences.json");
    match std::fs::read(path) {
        Ok(bytes) => {
            let value: Preferences = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            validate(&value)?;
            Ok(value)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn validate(value: &Preferences) -> Result<(), String> {
    if value.schema_version > 1 {
        return Err("settings schema is newer than this application".into());
    }
    if value
        .language
        .as_ref()
        .is_some_and(|l| !matches!(l.as_str(), "zh" | "en"))
    {
        return Err("invalid language".into());
    }
    if value
        .timezone
        .as_ref()
        .is_some_and(|z| z.parse::<chrono_tz::Tz>().is_err())
    {
        return Err("invalid timezone".into());
    }
    if value.codex_homes.len() > 32 || value.codex_homes.iter().any(|p| !p.is_absolute()) {
        return Err("log sources must be absolute paths (maximum 32)".into());
    }
    Ok(())
}

pub enum SaveError {
    Invalid(String),
    Storage(String),
}
pub fn save_patch(patch: serde_json::Value) -> Result<Preferences, SaveError> {
    static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = WRITE
        .lock()
        .map_err(|_| SaveError::Storage("settings writer unavailable".into()))?;
    let patch = patch
        .as_object()
        .ok_or_else(|| SaveError::Invalid("settings must be an object".into()))?;
    if patch.keys().any(|key| {
        !matches!(
            key.as_str(),
            "schema_version" | "language" | "timezone" | "codex_homes"
        )
    }) {
        return Err(SaveError::Invalid("unknown settings field".into()));
    }
    let mut current = serde_json::to_value(load().map_err(SaveError::Storage)?)
        .map_err(|e| SaveError::Storage(e.to_string()))?;
    for (key, value) in patch {
        current[key] = value.clone();
    }
    current["schema_version"] = patch
        .get("schema_version")
        .cloned()
        .unwrap_or(serde_json::json!(1));
    let value: Preferences =
        serde_json::from_value(current).map_err(|e| SaveError::Invalid(e.to_string()))?;
    validate(&value).map_err(SaveError::Invalid)?;
    crate::config::atomic_json(
        &crate::config::config_dir().join("preferences.json"),
        &value,
    )
    .map_err(|e| SaveError::Storage(e.to_string()))?;
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_newer_schema_and_invalid_settings() {
        for value in [
            Preferences {
                schema_version: 2,
                ..Default::default()
            },
            Preferences {
                timezone: Some("invalid".into()),
                ..Default::default()
            },
            Preferences {
                language: Some("xx".into()),
                ..Default::default()
            },
            Preferences {
                codex_homes: vec!["relative".into()],
                ..Default::default()
            },
        ] {
            assert!(validate(&value).is_err());
        }
        assert!(
            validate(&Preferences {
                timezone: Some("Asia/Shanghai".into()),
                ..Default::default()
            })
            .is_ok()
        );
    }
}
