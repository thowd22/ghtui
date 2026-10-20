//! History view (TASK-99): one GitKraken-style view for graph, file history and commit search.

pub mod detail;
pub mod filter;
pub mod lanes;
pub mod refs;

#[cfg(test)]
#[path = "history/view_tests.rs"]
mod view_tests;
