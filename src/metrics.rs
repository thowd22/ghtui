//! Prepared, bounded dashboard evidence. Native workflow integration remains in progress.
//! Snapshot health is not a historical trend; producers must supply category coverage.
mod adapters;
mod compute;
mod model;
pub use adapters::*;
pub use compute::*;
pub use model::*;
#[cfg(test)]
mod tests;
