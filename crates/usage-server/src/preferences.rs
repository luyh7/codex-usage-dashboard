use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
use usage_core::fsinfo::MAX_POLL_SECS;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub schema_version: u32,
    pub language: Option<String>,
    pub timezone: Option<String>,
    pub codex_homes: Vec<PathBuf>,
    /// Per-source polling keyed by the absolute log-source path.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub source_polling: BTreeMap<String, SourcePolling>,
}
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SourcePolling {
    /// Applies to network sources; values below 10 are raised to 10.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_interval_secs: Option<u64>,
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
    if value.source_polling.len() > 32
        || value
            .source_polling
            .keys()
            .any(|p| !std::path::Path::new(p).is_absolute())
    {
        return Err("source polling must use absolute log-source paths (maximum 32)".into());
    }
    if value
        .source_polling
        .values()
        .any(|p| p.poll_interval_secs.is_some_and(|s| s > MAX_POLL_SECS))
    {
        return Err("poll interval must not exceed one day".into());
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
            "schema_version" | "language" | "timezone" | "codex_homes" | "source_polling"
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
            Preferences {
                source_polling: BTreeMap::from([("relative".into(), SourcePolling::default())]),
                ..Default::default()
            },
            Preferences {
                source_polling: BTreeMap::from([(
                    absolute(),
                    SourcePolling {
                        poll_interval_secs: Some(MAX_POLL_SECS + 1),
                    },
                )]),
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
    fn absolute() -> String {
        std::env::temp_dir()
            .join(".codex")
            .to_string_lossy()
            .into_owned()
    }
    #[test]
    fn source_polling_round_trips_intervals_only() {
        let value: Preferences = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "source_polling": {(absolute()): {"poll_interval_secs": 5}}
        }))
        .unwrap();
        assert!(validate(&value).is_ok());
        let polling = &value.source_polling[&absolute()];
        assert_eq!(polling.poll_interval_secs, Some(5));
        let empty: SourcePolling = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.poll_interval_secs, None);
        // Unconfigured preferences serialize exactly as before.
        assert!(
            serde_json::to_value(Preferences::default())
                .unwrap()
                .get("source_polling")
                .is_none()
        );
        assert!(
            serde_json::from_value::<SourcePolling>(serde_json::json!({"network_mode": "on"}))
                .is_err()
        );
    }
}
