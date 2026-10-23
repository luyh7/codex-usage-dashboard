//! Synthetic benchmark harness; no automatic log discovery.
use serde::Deserialize;
use serde_json::json;
use std::{hint::black_box, path::PathBuf, time::Instant};
use usage_core::{
    PriceSchedule, RawEvent,
    parser::{self, LogSource},
    reader, usage,
};
#[derive(Deserialize)]
struct Quote {
    model: String,
    timestamp: String,
    input_tokens: i64,
    service_tier: String,
}
#[derive(Deserialize)]
struct Input {
    path: PathBuf,
    home: PathBuf,
    timeline: Vec<RawEvent>,
    quotes: Vec<Quote>,
}
fn measure<T>(mut f: impl FnMut() -> T) -> Vec<f64> {
    let mut samples = Vec::new();
    for i in 0..17 {
        let start = Instant::now();
        let result = f();
        black_box(&result);
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        if i >= 2 {
            samples.push(elapsed);
        }
        drop(result);
    }
    samples
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let input: Input = serde_json::from_reader(std::fs::File::open(&args[1]).unwrap()).unwrap();
    let pricing = PriceSchedule::bundled();
    let normalized = usage::normalize(&input.timeline).unwrap();
    let samples = match args[2].as_str() {
        "timeline" => measure(|| usage::normalize(&input.timeline).unwrap()),
        "pricing" => measure(|| {
            pricing
                .price_events(&normalized, "gpt-5", Default::default(), None, "")
                .unwrap()
        }),
        "lookup" => measure(|| {
            input
                .quotes
                .iter()
                .map(|q| {
                    pricing.quote(
                        &q.model,
                        Some(&q.timestamp),
                        Some(q.input_tokens),
                        &q.service_tier,
                    )
                })
                .collect::<Vec<_>>()
        }),
        "reader" => measure(|| {
            let mut rows = Vec::new();
            let stats = reader::read_rollout(&input.path, 0, None, |v| {
                rows.push(v);
                Ok(())
            })
            .unwrap();
            (rows, stats)
        }),
        "parse_file" => measure(|| {
            parser::parse_file(
                &input.path,
                "active",
                &LogSource {
                    id: "fixture".into(),
                    label: "Synthetic".into(),
                    home: input.home.clone(),
                },
                &pricing,
                None,
                None,
            )
            .unwrap()
        }),
        _ => panic!("unknown operation"),
    };
    let peak = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|s| s.starts_with("VmHWM:"))
                .map(str::to_owned)
        })
        .and_then(|s| {
            s.split_whitespace()
                .nth(1)
                .and_then(|v| v.parse::<u64>().ok())
        });
    println!("{}", json!({"samples_ms":samples,"peak_rss_kib":peak}));
}
