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
    file.seek(SeekFrom::Start(start))?;
    let mut reader = BufReader::with_capacity(128 * 1024, file.take(end.saturating_sub(start)));
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
