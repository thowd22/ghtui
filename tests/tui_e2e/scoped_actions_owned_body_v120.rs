//! Test-only causal navigation of THIS completed, owned Form. Never reads
//! partial parser state or joins invisible content/normalizes text semantics.
use crate::owned_form_prose_v120 as prose;
use crate::support::Session;
use std::time::{Duration, Instant};

pub(super) fn visible(
    session: &mut Session,
    mouse: bool,
    after: usize,
    title: &str,
    required: &[&str],
) {
    assert!(!required.is_empty());
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut scrolled_after = None;
    loop {
        session.poll_output(Duration::from_millis(1));
        let end = session.completed_cells_end();
        if end > after && scrolled_after.is_none_or(|previous| end > previous) {
            if let Some(body) = session
                .completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, title))
            {
                // A last-child journal record is NOT publication. Never
                // navigate the still-busy retained underlay.
                if !body.contains("[Esc Cancel]") {
                    if required
                        .iter()
                        .all(|text| prose::adjacent_literal(&body, text))
                    {
                        return;
                    }
                    let (x, y) =
                        session.wait_for_owned_completed_action(title, "[Keep draft / Close]", &[]);
                    scrolled_after = Some(end);
                    if mouse {
                        session.wheel(x, y.saturating_sub(3), true);
                    } else {
                        session.send_key(b"\x1b[6~");
                    }
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "Exact owned completed body must visibly show {required:?} after one scroll per new frame: {title}\n{}",
            session.screen()
        );
    }
}
