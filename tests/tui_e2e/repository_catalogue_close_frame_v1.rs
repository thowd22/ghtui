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
        // The main key bar's whole `key label` tokens (two-space separated), never a
        // bracketed modal row; Commands is `: Cmds` below 100 columns.
        && {
            let token = |label: &str| {
                footer.match_indices(label).any(|(at, _)| {
                    (at == 0 || footer[..at].ends_with(' '))
                        && (footer[at + label.len()..].starts_with("  ")
                            || footer[at + label.len()..].trim().is_empty())
                })
            };
            ["? Help", "r Refresh", "s Settings", "q Quit"]
                .into_iter()
                .all(token)
                && (token(": Commands") || token(": Cmds"))
        }
}
