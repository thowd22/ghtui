//! Catalogue-specific acknowledgement of the restored actual main frame.
//! Completed geometry/parsing comes from the existing shared PTY framework.
pub fn restored_main(
    frame: Option<&vt100::Screen>,
    completed_end: usize,
    before_close: usize,
) -> bool {
    if completed_end <= before_close {
        return false;
    }
    let Some(frame) = frame else { return false };
    let text = frame.contents();
    let footer = frame.rows(0, frame.size().1).last().unwrap_or_default();
    text.contains("Repository • focused view")
        && !text.contains("Repositories • explicit discovery")
        && !text.contains("Action unavailable • explanation only")
        && [
            "[? Help]",
            "[: Commands]",
            "[r Refresh]",
            "[s Settings]",
            "[q Quit]",
        ]
        .iter()
        .all(|label| footer.contains(label))
}
