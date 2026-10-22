use crate::Error;
use regex::bytes::Regex;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::Path,
    sync::LazyLock,
};

static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\{"timestamp":"([0-9T:.+Z-]+)","type":"(response_item|event_msg)","payload":\{"type":"([a-zA-Z0-9_-]+)"(?:,|\})"#).unwrap()
});

pub fn is_compressed(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "zst")
        && path.file_stem().is_some_and(|stem| {
            Path::new(stem)
                .extension()
                .is_some_and(|ext| ext == "jsonl")
        })
}

#[derive(Debug, Default, Serialize)]
pub struct ReadStats {
    pub bytes_read: u64,
    pub fast_skipped_line_count: u64,
    pub fast_skipped_bytes: u64,
}

fn project(line: &[u8]) -> Option<Value> {
    if line.len() < 4096 || !line.ends_with(b"}}\n") {
        return None;
    }
    let captures = HEADER.captures(line)?;
    let outer = std::str::from_utf8(&captures[2]).ok()?;
    let inner = std::str::from_utf8(&captures[3]).ok()?;
    if !matches!(
        (outer, inner),
        (
            "response_item",
            "custom_tool_call_output"
                | "function_call_output"
                | "image_generation_call"
                | "reasoning"
                | "tool_search_output"
        ) | ("event_msg", "image_generation_end")
    ) {
        return None;
    }
    Some(
        json!({"timestamp":std::str::from_utf8(&captures[1]).ok()?,"type":outer,"payload":{"type":inner}}),
    )
}

/// Streaming, bounded by a stat captured before the read. The callback owns
/// each transient raw object; parsed history need not retain JSON objects.
/// A valid final JSON object without newline is consumed, an incomplete final
/// JSON value is left for the next append. Encoding errors use U+FFFD.
pub fn read_rollout(
    path: &Path,
    start: u64,
    end: Option<u64>,
    mut consume: impl FnMut(Value) -> Result<(), Error>,
) -> Result<ReadStats, Error> {
    let mut file = File::open(path)?;
    let end = end.unwrap_or(file.metadata()?.len());
    // `end` bounds physical bytes, whereas `start` and ReadStats use decoded
    // JSONL offsets. Compressed files are reparsed on change by the index.
    let input: Box<dyn Read> = if is_compressed(path) {
        let mut decoded = zstd::stream::read::Decoder::new(file.take(end))?;
        std::io::copy(&mut decoded.by_ref().take(start), &mut std::io::sink())?;
        Box::new(decoded)
    } else {
        file.seek(SeekFrom::Start(start))?;
        Box::new(file.take(end.saturating_sub(start)))
    };
    let mut reader = BufReader::with_capacity(128 * 1024, input);
    let mut first = start == 0;
    let mut stats = ReadStats::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let complete = line.ends_with(b"\n");
        if let Some(record) = project(&line) {
            stats.bytes_read += line.len() as u64;
            stats.fast_skipped_line_count += 1;
            stats.fast_skipped_bytes += line.len() as u64;
            first = false;
            consume(record)?;
            continue;
        }
        let bytes = if first {
            line.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&line)
        } else {
            &line
        };
        first = false;
        let decoded = String::from_utf8_lossy(bytes);
        let text = decoded.trim();
        if text.is_empty() {
            if complete {
                stats.bytes_read += line.len() as u64;
            }
            continue;
        }
        match serde_json::from_str::<Value>(text) {
            Ok(record) => {
                stats.bytes_read += line.len() as u64;
                if record.is_object() {
                    consume(record)?;
                }
            }
            Err(_) if !complete => break,
            Err(_) => {
                stats.bytes_read += line.len() as u64;
                consume(json!({"__parse_error__":true}))?;
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_frames_respect_physical_bounds_and_decoded_offsets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout.jsonl.zst");
        let first = b"\xef\xbb\xbf{\"n\":1}\n";
        let second = b"{\"n\":2}\n";
        let mut bytes = zstd::stream::encode_all(first.as_slice(), 1).unwrap();
        let bound = bytes.len() as u64;
        bytes.extend(zstd::stream::encode_all(second.as_slice(), 1).unwrap());
        let complete = bytes.len() as u64;
        bytes.extend(b"unpublished trailing bytes");
        std::fs::write(&path, bytes).unwrap();
        let mut rows = Vec::new();
        let stats = read_rollout(&path, 0, Some(bound), |r| {
            rows.push(r);
            Ok(())
        })
        .unwrap();
        assert_eq!(rows, vec![json!({"n":1})]);
        assert_eq!(stats.bytes_read, first.len() as u64);
        rows.clear();
        read_rollout(&path, stats.bytes_read, Some(complete), |r| {
            rows.push(r);
            Ok(())
        })
        .unwrap();
        assert_eq!(rows, vec![json!({"n":2})]);
        assert!(read_rollout(&path, 0, None, |_| Ok(())).is_err());
        let truncated = zstd::stream::encode_all(second.as_slice(), 1).unwrap();
        std::fs::write(&path, &truncated[..truncated.len() - 1]).unwrap();
        assert!(read_rollout(&path, 0, None, |_| Ok(())).is_err());
    }
}
