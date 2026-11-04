//! I.R.I.S. domain model and storage.

pub mod model;
pub mod store;

pub use model::*;
pub use store::Store;

/// Milliseconds since the Unix epoch; every timestamp in I.R.I.S. uses this.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// New time-ordered id (UUID v7), so sorting by id also sorts by creation.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
