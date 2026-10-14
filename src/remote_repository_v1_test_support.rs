//! One library fixture module; no duplicate support registration.
use crate as model;
use crate::test_support::{Fixture, GhStep};
#[path = "../tests/support/remote_repository_v1.rs"]
mod implementation;
pub(crate) use implementation::*;
