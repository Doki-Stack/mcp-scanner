// Not wired into any route yet — the scan/get-context handlers land in
// later tasks (B11/B12). Remove once they consume these.
#![allow(dead_code, unused_imports)]

pub mod context;
pub mod scan;

pub use context::{GetContextRequest, GetContextResponse};
pub use scan::{ScanRequest, ScanResponse};
