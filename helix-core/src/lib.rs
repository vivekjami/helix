#![deny(clippy::unwrap_used)]
#![deny(clippy::panic)]
#![deny(clippy::missing_errors_doc)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

pub mod error;
pub mod fingerprint;
pub mod freshness;
pub mod lsh;
pub mod simd;

pub use error::HelixError;
pub use fingerprint::fingerprint_document;
pub use freshness::{FreshnessResult, FreshnessStore};
pub use lsh::{DedupStatus, LshConfig, LshIndex};
pub use simd::simd_path;
