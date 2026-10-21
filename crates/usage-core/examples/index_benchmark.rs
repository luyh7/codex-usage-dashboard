//! End-to-end synthetic index/cache measurements, isolated from any user roots.
use chrono::Utc;
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};
use usage_core::{
    PriceSchedule,
    index::{Index, IndexConfig, Snapshot},
    parser::LogSource,
    query::Period,
    view::View,
};
fn peak() -> u64 {
    fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find(|s| s.starts_with("VmHWM:"))
        .and_then(|s| s.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}
fn view(snapshot: &Arc<Snapshot>) -> View {
    View::new(
        snapshot,
        Period::bounds("all", None, None, chrono_tz::UTC, Utc::now()).unwrap(),
        chrono_tz::UTC,
        &PriceSchedule::bundled(),
    )
    .unwrap()
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let home = PathBuf::from(&args[1]);
    let database = PathBuf::from(&args[2]);
    let operation = &args[3];
    let source = LogSource {
        id: "fixture".into(),
        label: "Synthetic".into(),
        home: home.clone(),
    };
    if operation == "remote" {
        let start = Instant::now();
        let mut store =
            usage_core::remote::Store::new(home, "local-fixture".into(), chrono_tz::UTC);
        let (_, remotes) = store.refresh(&PriceSchedule::bundled()).unwrap();
        let sessions = remotes
            .into_iter()
            .flat_map(|r| r.sessions.clone())
            .collect();
        let snapshot = Arc::new(Snapshot {
            revision: 1,
            generated_at: usage_core::timestamp::format(Utc::now()),
            sessions,
        });
        let view = view(&snapshot);
        let summary = view.summary().unwrap();
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        println!(
            "{}",
            json!({"samples_ms":[elapsed],"peak_rss_kib":peak(),"sessions":summary.session_count,"tokens":summary.usage.total_tokens})
        );
        return;
    }
    if operation == "inventory" {
        let mut inventory = usage_core::inventory::Inventory::new(vec![source]);
        let mut samples = Vec::new();
        let mut count = 0;
        for i in 0..17 {
            let started = Instant::now();
            let files = inventory.discover().unwrap();
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            count = files.len();
            if i >= 2 {
                samples.push(elapsed)
            }
        }
        println!(
            "{}",
            json!({"samples_ms":samples,"peak_rss_kib":peak(),"files":count,"metrics":inventory.metrics})
        );
        return;
    }
    let config = IndexConfig {
        database,
        sources: vec![source],
    };
    let started = Instant::now();
    let mut index = Index::open(config).unwrap();
    index.refresh(|_| {}).unwrap();
    let initial = view(&index.snapshot());
    let initial_summary = initial.summary().unwrap();
    let initial_elapsed = started.elapsed().as_secs_f64() * 1000.;
    drop(initial);
    let mut samples = Vec::new();
    if ["cold", "restore"].contains(&operation.as_str()) {
        samples.push(initial_elapsed)
    } else if operation == "warm" {
        for i in 0..17 {
            let started = Instant::now();
            assert!(index.refresh(|_| {}).unwrap().is_none());
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            if i >= 2 {
                samples.push(elapsed)
            }
        }
    } else if operation == "append" {
        let path = home.join("sessions/rollout-0000.jsonl");
        let original = fs::read(&path).unwrap();
        let original_time = fs::metadata(&path).unwrap().modified().unwrap();
        for i in 0..17 {
            let n = 81 + i;
            let usage = json!({"input_tokens":n*1000,"cached_input_tokens":n*600,"output_tokens":n*120,"reasoning_output_tokens":n*40,"total_tokens":n*1120});
            let row = json!({"timestamp":format!("2026-10-01T00:02:{i:02}Z"),"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":usage,"last_token_usage":{"input_tokens":1000,"cached_input_tokens":600,"output_tokens":120,"reasoning_output_tokens":40,"total_tokens":1120}}}});
            writeln!(
                OpenOptions::new().append(true).open(&path).unwrap(),
                "{row}"
            )
            .unwrap();
            let started = Instant::now();
            index.refresh(|_| {}).unwrap();
            let current = view(&index.snapshot());
            let summary = current.summary().unwrap();
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            assert_eq!(
                summary.usage.total_tokens,
                initial_summary.usage.total_tokens + (i + 1) * 1120
            );
            if i >= 2 {
                samples.push(elapsed)
            }
        }
        fs::write(&path, original).unwrap();
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(original_time))
            .unwrap();
    } else if operation == "reprice" {
        for i in 0..17 {
            let mut asset: serde_json::Value =
                serde_json::from_str(include_str!("../data/prices-v1.json")).unwrap();
            for schedule in asset["schedules"].as_array_mut().unwrap() {
                schedule[1]["gpt-5"]["input"] = json!(if i % 2 == 0 { 2.0 } else { 1.25 });
            }
            let asset = asset.to_string();
            let started = Instant::now();
            index.replace_pricing(&asset).unwrap();
            index.refresh(|_| {}).unwrap();
            let current = view(&index.snapshot());
            let summary = current.summary().unwrap();
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            assert_eq!(
                summary.usage.total_tokens,
                initial_summary.usage.total_tokens
            );
            if i >= 2 {
                samples.push(elapsed)
            }
        }
    }
    println!(
        "{}",
        json!({"samples_ms":samples,"peak_rss_kib":peak(),"sessions":initial_summary.session_count,"tokens":initial_summary.usage.total_tokens,"metrics":index.metrics})
    );
}
