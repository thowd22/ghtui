//! External tool adapters. GitHub credentials and transport belong to `gh`.
pub mod gh;
pub mod local_git_navigation_read_v1;
#[doc(hidden)]
pub mod owned_pty_viewer_v4;
pub(crate) mod remote_git_read_window_v1;
pub mod runner;
