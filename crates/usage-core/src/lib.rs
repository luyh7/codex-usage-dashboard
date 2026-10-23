//! Host-independent Codex usage domain. HTTP, UI and process lifecycle live in
//! adapters. Counters use checked signed 64-bit arithmetic; malformed or
//! overflowing input is reported instead of silently wrapping.
mod content;
pub mod index;
pub mod inventory;
pub mod normalize;
pub mod parser;
pub mod pricing;
pub mod project;
pub mod query;
pub mod reader;
pub mod remote;
pub mod runtime;
pub mod speed;
pub mod timestamp;
pub mod usage;
pub mod view;
pub mod wire;

pub use pricing::{PriceEntry, PriceSchedule, PriceSegment, Pricing};
pub use usage::{NormalizedEvent, RawEvent, Usage};

#[derive(Debug)]
pub enum Error {
    CounterOverflow,
    Io(std::io::Error),
    Database(rusqlite::Error),
    Json(serde_json::Error),
    InvalidData(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CounterOverflow => f.write_str("token counter exceeds signed 64-bit range"),
            Self::Io(e) => e.fmt(f),
            Self::Database(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::InvalidData(e) => f.write_str(e),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database(e)
    }
}
