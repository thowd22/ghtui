//! Prepared, bounded dashboard evidence. Native workflow integration remains in progress.
//! Snapshot health is not a historical trend; producers must supply category coverage.
mod adapters;
mod compute;
mod model;
mod threads;
pub use adapters::*;
pub use compute::*;
pub use model::*;
pub use threads::*;
#[cfg(test)]
mod tests;
