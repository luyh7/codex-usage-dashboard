//! Lossless snapshot transport; the JSON schema remains unchanged.
use axum::body::Bytes;
use flate2::{Compression, read::MultiGzDecoder, write::GzEncoder};
use serde::{Serialize, Serializer, ser::SerializeMap};
use serde_json::Value;
use std::{
    fs,
    io::{BufWriter, Read, Write},
    path::Path,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};
use usage_core::{
    Error,
    parser::LogSource,
    query::{DailyUsage, SessionSummary, Summary},
    view::{QueryError, View},
};

pub const MAX_BYTES: usize = 512 * 1024 * 1024;

/// A portable, recognizable default name; redacted exports must not expose the device.
pub fn filename(label: &str, device: &str, redacted: bool) -> String {
    if redacted {
        return "cud-shared.json.gz".into();
    }
    let mut safe_label = String::new();
    for ch in label.chars() {
        if ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            if safe_label.len() + ch.len_utf8() > 96 {
                break;
            }
            safe_label.push(ch);
        } else if !safe_label.is_empty() && !safe_label.ends_with('-') && safe_label.len() < 96 {
            safe_label.push('-');
        }
    }
    let safe_label = safe_label.trim_matches(['-', '_', '.']);
    if safe_label.is_empty() {
        format!("cud-{device}.json.gz")
    } else {
        format!("cud-{safe_label}-{device}.json.gz")
    }
}

pub struct ExportJob {
    pub redacted: bool,
    pub id: String,
    pub revision: u64,
    pub percent: AtomicUsize,
    pub result: Mutex<Option<Result<Bytes, String>>>,
}
impl ExportJob {
    pub fn status(&self) -> Value {
        let result = self.result.lock().unwrap();
        let (phase, error) = match result.as_ref() {
            Some(Ok(_)) => ("ready", None),
            Some(Err(error)) => ("error", Some(error)),
            None if self.percent.load(Ordering::Relaxed) < 5 => ("preparing", None),
            None => ("compressing", None),
        };
        serde_json::json!({"id":self.id,"percent":self.percent.load(Ordering::Relaxed),"phase":phase,"error":error})
    }
}

#[derive(Serialize)]
pub struct Document<'a> {
    schema: &'static str,
    version: u8,
    exported_at: String,
    device: Value,
    snapshot: Data<'a>,
}
#[derive(Serialize)]
struct Data<'a> {
    generated_at: &'a str,
    codex_home: String,
    codex_sources: Vec<Value>,
    summary: Summary,
    sessions: Vec<&'a SessionSummary>,
    details_by_uid: Details<'a>,
    daily_usage: &'a [DailyUsage],
}
struct Details<'a> {
    view: &'a View,
    progress: Option<&'a AtomicUsize>,
}
impl Serialize for Details<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let total = self
            .view
            .details()
            .map(|d| d.timeline.len() + 1)
            .sum::<usize>()
            .max(1);
        let mut done = 0;
        let mut map = serializer.serialize_map(Some(self.view.details().count()))?;
        for detail in self.view.details() {
            map.serialize_entry(&detail.identity.uid, detail)?;
            done += detail.timeline.len() + 1;
            if let Some(progress) = self.progress {
                progress.store(5 + 85 * done / total, Ordering::Relaxed);
            }
        }
        map.end()
    }
}
pub fn document<'a>(
    view: &'a View,
    device: Value,
    sources: &[LogSource],
    progress: Option<&'a AtomicUsize>,
) -> Result<Document<'a>, QueryError> {
    let summary = view.summary()?;
    if let Some(progress) = progress {
        progress.store(5, Ordering::Relaxed);
    }
    Ok(Document {
        schema: usage_core::remote::SCHEMA,
        version: 1,
        exported_at: usage_core::timestamp::format(chrono::Utc::now()),
        device,
        snapshot: Data {
            generated_at: &view.generated_at,
            codex_home: crate::config::homes_display(sources),
            codex_sources: crate::config::source_rows(sources),
            summary,
            sessions: view.summaries().collect(),
            details_by_uid: Details { view, progress },
            daily_usage: &view.daily_usage,
        },
    })
}

pub fn gzip(value: &impl Serialize) -> Result<Vec<u8>, Error> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut buffered = BufWriter::with_capacity(256 * 1024, &mut encoder);
        serde_json::to_writer(&mut buffered, value)?;
        buffered.flush()?;
    }
    Ok(encoder.finish()?)
}

pub fn write_gzip(path: &Path, value: &impl Serialize) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::InvalidData("file has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    let mut encoder = GzEncoder::new(&mut file, Compression::default());
    {
        let mut buffered = BufWriter::with_capacity(256 * 1024, &mut encoder);
        serde_json::to_writer(&mut buffered, value)?;
        buffered.flush()?;
    }
    encoder.finish()?.flush()?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn decode(bytes: &[u8]) -> Result<Value, Error> {
    decode_limited(bytes, MAX_BYTES)
}

fn decode_limited(bytes: &[u8], limit: usize) -> Result<Value, Error> {
    if bytes.len() > limit {
        return Err(Error::InvalidData(
            "snapshot exceeds size limit (512 MiB)".into(),
        ));
    }
    let mut decoded = Vec::new();
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        MultiGzDecoder::new(bytes)
            .take(limit as u64 + 1)
            .read_to_end(&mut decoded)?;
        if decoded.len() > limit {
            return Err(Error::InvalidData(
                "decompressed snapshot exceeds size limit (512 MiB)".into(),
            ));
        }
        decoded.as_slice()
    } else {
        bytes
    };
    Ok(serde_json::from_slice(
        bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn export_names_are_readable_portable_and_do_not_identify_shared_exports() {
        assert_eq!(
            filename("研发 Mac/Desk", "mac-12345678", false),
            "cud-研发-Mac-Desk-mac-12345678.json.gz"
        );
        assert_eq!(
            filename("..\\/:*?\"<>|\r\n", "win-12345678", false),
            "cud-win-12345678.json.gz"
        );
        assert_eq!(
            filename("机密电脑", "mac-12345678", true),
            "cud-shared.json.gz"
        );
        assert!(filename(&"设备".repeat(100), "mac-12345678", false).len() < 140);
    }

    #[test]
    fn lossless_json_and_gzip_including_unicode_and_bom() {
        let value = json!({"schema":"cousash.remote-snapshot","snapshot":{"timeline":[{"tokens":123,"title":"中文快照"}]}});
        let plain = serde_json::to_vec(&value).unwrap();
        assert_eq!(decode(&plain).unwrap(), value);
        assert_eq!(
            decode(&[b"\xef\xbb\xbf".as_slice(), &plain].concat()).unwrap(),
            value
        );
        let compressed = gzip(&value).unwrap();
        assert_eq!(decode(&compressed).unwrap(), value);
        assert!(decode(&compressed[..compressed.len() - 3]).is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snapshot.json.gz");
        write_gzip(&path, &value).unwrap();
        assert_eq!(decode(&fs::read(path).unwrap()).unwrap(), value);
    }

    #[test]
    fn rejects_invalid_and_oversized_data() {
        assert!(decode(b"not json").is_err());
        assert!(decode(&[0x1f, 0x8b, 0]).is_err());
        let value = json!({"padding":"x".repeat(4096)});
        assert!(decode_limited(&gzip(&value).unwrap(), 1024).is_err());
        assert!(decode_limited(&serde_json::to_vec(&value).unwrap(), 1024).is_err());
    }

    #[test]
    fn borrowed_document_preserves_all_session_fields_and_reports_work() {
        use std::sync::Arc;
        use usage_core::{
            PriceSchedule, RawEvent, Usage,
            index::{IndexedSession, Snapshot},
            parser::{SessionDetail, SessionIdentity},
            query::Period,
        };
        let stamp = "2026-10-22T01:00:00Z";
        let detail = SessionDetail {
            identity: SessionIdentity {
                uid: "one".into(),
                session_id: "one".into(),
                ..Default::default()
            },
            title: "中文快照".into(),
            model: "gpt-5".into(),
            start_at: stamp.into(),
            end_at: stamp.into(),
            timeline: vec![RawEvent {
                timestamp: Some(stamp.into()),
                model: "gpt-5".into(),
                total_token_usage: Usage {
                    input_tokens: 10,
                    output_tokens: 2,
                    total_tokens: 12,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        };
        let expected = serde_json::to_value(&detail).unwrap();
        let source = Snapshot {
            revision: 1,
            generated_at: stamp.into(),
            sessions: vec![IndexedSession::from_detail(Arc::new(detail)).unwrap()],
        };
        let period = Period::bounds(
            "all",
            None,
            None,
            chrono_tz::UTC,
            usage_core::timestamp::parse(stamp).unwrap(),
        )
        .unwrap();
        let view = View::new(&source, period, chrono_tz::UTC, &PriceSchedule::bundled()).unwrap();
        let progress = AtomicUsize::new(1);
        let doc = document(
            &view,
            serde_json::json!({"short_code":"test"}),
            &[],
            Some(&progress),
        )
        .unwrap();
        let exported = decode(&gzip(&doc).unwrap()).unwrap();
        assert_eq!(exported["snapshot"]["details_by_uid"]["one"], expected);
        assert_eq!(
            exported["snapshot"]["sessions"],
            serde_json::to_value(view.summaries().collect::<Vec<_>>()).unwrap()
        );
        assert_eq!(
            exported["snapshot"]["summary"],
            serde_json::to_value(view.summary().unwrap()).unwrap()
        );
        assert_eq!(
            exported["snapshot"]["daily_usage"],
            serde_json::to_value(&view.daily_usage).unwrap()
        );
        assert_eq!(progress.load(Ordering::Relaxed), 90);
    }
}
