//! TASK-95 on a real PR Conversation: each actor row is the selection marker, the author's
//! filled initials chip, exactly one blank cell, then `@login text`. Row text painted before the
//! chip never shows through it ("OCmm@octocat comment #9" / "RE d@requester PR description").
//! Real cells from the compiled binary at 120x40 and 80x24, by keyboard and by mouse.
use super::{
    configured, details, initial, local_git_witness, mutation_count, open, records, select_comment,
    starter,
};
use crate::support::Session;
use ghtui::identity::{ActorIdentity, ActorKind, InitialsChip};
use ghtui::theme::CATPPUCCIN_MOCHA;
use std::time::{Duration, Instant};

fn octocat() -> ActorIdentity {
    ActorIdentity {
        host: Some("github.invalid".into()),
        login: Some("octocat".into()),
        name: String::new(),
        email: None,
        avatar_url: None,
        kind: ActorKind::User,
        git_role: None,
        host_associated: true,
    }
}

/// Both Conversation actor rows open with a clean `OC @octocat` chip in the identity hue.
fn clean_chips(frame: &vt100::Screen) -> bool {
    let hex = CATPPUCCIN_MOCHA.lanes()[InitialsChip::slot(&octocat())];
    let hue = vt100::Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    let contents = frame.contents();
    let mut seen = [false, false];
    for (row, line) in contents.lines().enumerate() {
        for (index, text) in ["@octocat PR description", "@octocat comment #9"]
            .iter()
            .enumerate()
        {
            let Some(at) = line.find(text) else {
                continue;
            };
            let column = line[..at].chars().count() as u16;
            let Some(chip) = column.checked_sub(3) else {
                return false;
            };
            let filled = "OC".chars().enumerate().all(|(offset, letter)| {
                frame
                    .cell(row as u16, chip + offset as u16)
                    .is_some_and(|cell| {
                        cell.contents() == letter.to_string() && cell.bgcolor() == hue
                    })
            });
            // Exactly one blank, unfilled cell between the chip and the login.
            let gap = frame
                .cell(row as u16, chip + 2)
                .is_some_and(|cell| cell.contents().trim().is_empty() && cell.bgcolor() != hue);
            if !filled || !gap || !line.contains(&format!("OC {text}")) {
                return false;
            }
            seen[index] = true;
        }
    }
    seen == [true, true]
        && !contents.contains("OCmm")
        && !contents.contains("OC d@")
        && !contents.contains("[OC]")
}

fn wait_chips(session: &mut Session, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(5));
        if session.completed_cells_screen().is_some_and(clean_chips) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Conversation chips not clean: {label}\n{}",
            session.screen()
        );
    }
}

fn journey(mouse: bool, size: (u16, u16), resized: (u16, u16)) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let mut steps = initial();
    steps.extend(details(vec![starter()]));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, size.0, size.1);
    open(&mut session, mouse);
    wait_chips(&mut session, "opened Conversation");
    session.capture_completed_terminal_v1(if mouse {
        "design-chips-v95-conversation-mouse"
    } else {
        "design-chips-v95-conversation-keyboard"
    });
    // Selecting a comment keeps every chip and its single gap.
    select_comment(&mut session, mouse, "Existing starter comment");
    wait_chips(&mut session, "after selecting a comment");
    session.resize(resized.0, resized.1);
    wait_chips(&mut session, "after resize");
    // Read-only browsing: every scripted read ran, no mutation, the checkout is untouched.
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, steps.len())),
        0
    );
    assert_eq!(local_git_witness(&fixture), git);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_chips_v95_keyboard_pr_conversation_rows_have_one_clean_gap_120x40_then_80x24() {
    journey(false, (120, 40), (80, 24));
}

#[test]
fn design_chips_v95_mouse_pr_conversation_rows_have_one_clean_gap_80x24_then_120x40() {
    journey(true, (80, 24), (120, 40));
}
