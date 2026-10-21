//! TASK-115 on the actual original Conversation: the persistent Add a comment box, a card's
//! React, Edit and Quote reply buttons (mouse) and their keys `n`, `+`, `E`, `Q` (keyboard)
//! open the SAME original composers the PR actions menu opens, with the same strict fresh
//! reads, Preview, explicit Confirm, receipts, SQL projection and search corpus. Quote reply
//! pastes the quoted comment into the empty New comment body; the published body is exact.
use super::*;

const CREATED: &str = "Comment before native edit";
const QUOTED_TEXT: &str = "Thanks";
const QUOTE_BODY: &str = "> @octocat wrote:\n> Edited exact comment 雪\n\nThanks";

/// Click the `label` button of the card whose body line reads `body`, scrolling the
/// Conversation with the wheel until the card's button row is painted.
fn click_card_button(s: &mut Session, body: &str, label: &str) {
    for _ in 0..24 {
        s.poll_output(Duration::from_millis(10));
        let screen = s.screen();
        let lines = screen.lines().collect::<Vec<_>>();
        if let Some(at) = lines.iter().position(|line| line.contains(body)) {
            if let Some((offset, line)) = lines
                .iter()
                .enumerate()
                .skip(at + 1)
                .take(4)
                .find(|(_, line)| line.contains(label))
            {
                let column = line[..line.find(label).unwrap()].chars().count() as u16;
                s.click(column + 1, offset as u16);
                return;
            }
        }
        let frame = s.completed_cursorless_end();
        s.wheel(30, 12, true);
        s.wait_for_new_cursorless_condition(
            frame,
            crate::support::main_footer_visible,
            "One Conversation wheel scroll toward a card's buttons",
            Duration::from_secs(8),
        );
    }
    panic!("{label} of the card {body:?} never painted\n{}", s.screen());
}

/// Opens `command`'s composer from the new controls: the box or a card button by mouse,
/// the bar or card key by keyboard. The comment is selected first for card controls.
fn open_from_cards(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: PrCommand,
    quote: bool,
    body: Option<&str>,
    end: usize,
) {
    if let Some(body) = body {
        select_current(s, mouse, body);
    } else {
        let after = s.completed_cursorless_end();
        choose(s, mouse, "[c Conversation]", b"c");
        s.wait_for_new_cursorless_condition(
            after,
            |screen| {
                crate::support::main_footer_visible(screen)
                    && screen.contents().contains("✎ Add a comment…")
                    && screen.contents().contains("[n Comment]")
            },
            "Conversation with its action bar and Add a comment box",
            Duration::from_secs(8),
        );
    }
    let frame = s.completed_cells_end();
    let (label, key): (&str, &[u8]) = match (command, quote) {
        (PrCommand::NewComment, false) => ("✎ Add a comment…", b"n"),
        (PrCommand::NewComment, true) => ("[Q Quote reply]", b"Q"),
        (PrCommand::AddReaction, _) => ("[+ React]", b"+"),
        (PrCommand::EditComment, _) => ("[E Edit]", b"E"),
        _ => unreachable!(),
    };
    if mouse {
        match body {
            Some(body) => click_card_button(s, body, label),
            None => s.click_visible_action(label),
        }
    } else {
        s.send_key(key);
    }
    s.wait_for_command_count(f, end);
    form_frame(s, frame, &title(command), controls(command));
}

#[derive(Clone, Copy)]
struct Region {
    opened: usize,
    preview: usize,
    end: usize,
}

/// (command, quote reply, selected body before, body published)
const FLOW: [(PrCommand, bool, Option<&str>, &str); 4] = [
    (PrCommand::NewComment, false, None, CREATED),
    (PrCommand::AddReaction, false, Some(CREATED), CREATED),
    (PrCommand::EditComment, false, Some(CREATED), EDITED_BODY),
    (PrCommand::NewComment, true, Some(EDITED_BODY), QUOTE_BODY),
];

fn cards_journey(mouse: bool) {
    let created = actual_comment(10, CREATED);
    let edited = actual_comment(10, EDITED_BODY);
    let quoted = actual_comment(11, QUOTE_BODY);
    let owned = reaction(501, "heart", 42);
    let mut steps = initial_steps(true);
    let initial = steps.len();
    let mut regions = Vec::new();
    for (index, (command, _, _, body)) in FLOW.into_iter().enumerate() {
        let old = if index < 3 { &created } else { &edited };
        for opening in [true, false, false] {
            steps.extend(current_phase(
                command,
                opening,
                old,
                json!("SUBSCRIBED"),
                true,
                &[],
            ));
            if opening {
                regions.push(Region {
                    opened: steps.len(),
                    preview: 0,
                    end: 0,
                });
            } else if regions.last().unwrap().preview == 0 {
                regions.last_mut().unwrap().preview = steps.len();
            }
        }
        let receipt = match index {
            0 | 1 => &created,
            2 => &edited,
            _ => &quoted,
        };
        steps.extend(current_receipt(
            command,
            receipt,
            body,
            std::slice::from_ref(&owned),
        ));
        steps.extend(visibility_with_comments_v19(match index {
            0 | 1 => json!([comment(501, OLD), created.clone()]),
            2 => json!([comment(501, OLD), edited.clone()]),
            _ => json!([comment(501, OLD), edited.clone(), quoted.clone()]),
        }));
        regions.last_mut().unwrap().end = steps.len();
    }
    let f = resource_configured(&steps);
    seed_independent(&f);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
    let kept = protected(&f);
    for (index, ((command, quote, selected, body), region)) in
        FLOW.into_iter().zip(regions).enumerate()
    {
        open_from_cards(&mut s, &f, mouse, command, quote, selected, region.opened);
        if quote {
            // The quoted comment is already in the empty body; the reply is typed after it.
            s.wait_for_screen("> @octocat wrote:");
            s.wait_for_screen("> Edited exact comment 雪");
            s.paste(QUOTED_TEXT);
            s.wait_for_screen(QUOTED_TEXT);
        } else {
            if command == PrCommand::EditComment {
                s.wait_for_screen(CREATED);
            }
            fill(&mut s, mouse, command, body);
        }
        preview_current(&mut s, &f, mouse, command, region.preview);
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            index,
            "Nothing is published before Confirm"
        );
        refreshed(&mut s, &f, mouse, command, region.end);
        let expected = match index {
            0 | 1 => vec![(501, OLD), (10, CREATED)],
            2 => vec![(501, OLD), (10, EDITED_BODY)],
            _ => vec![(501, OLD), (10, EDITED_BODY), (11, QUOTE_BODY)],
        };
        assert_projection_v19(&f, &expected, &[], (index + 1) as u64);
        exact(&s, &f, &steps[..region.end]);
        assert_eq!(protected(&f), kept);
        assert_eq!(git_state(&f), git);
    }
    // The published quote reply renders as a quote inside its own card.
    select_current(&mut s, mouse, "@octocat wrote:");
    s.wait_for_screen("│ @octocat wrote:");
    // The selected first quote line may be the last visible row: one Down (keys) or one
    // wheel notch (mouse) brings the rest of the card into view.
    for _ in 0..4 {
        if s.screen().contains("│ Edited exact comment 雪") {
            break;
        }
        let frame = s.completed_cursorless_end();
        if mouse {
            s.wheel(30, 12, true);
        } else {
            s.send_key(b"\x1b[B");
        }
        s.wait_for_new_cursorless_condition(
            frame,
            crate::support::main_footer_visible,
            "One Conversation scroll toward the rest of the quote reply card",
            Duration::from_secs(8),
        );
    }
    s.wait_for_screen("│ Edited exact comment 雪");
    confirm_receipts(
        &f,
        &[
            (command_key(PrCommand::NewComment), "10"),
            (command_key(PrCommand::AddReaction), "501"),
            (command_key(PrCommand::EditComment), "10"),
            (command_key(PrCommand::NewComment), "11"),
        ],
    );
    let methods = commands(&s, &f)
        .into_iter()
        .filter_map(|row| {
            let args = row["args"].as_array()?;
            let i = args.iter().position(|v| v == "--method")?;
            let m = args.get(i + 1)?.as_str()?;
            matches!(m, "POST" | "PATCH" | "DELETE")
                .then(|| (args[1].as_str().unwrap().to_owned(), m.to_owned()))
        })
        .filter(|(endpoint, _)| endpoint != "graphql")
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        vec![
            (format!("repos/{FIRST}/issues/7/comments"), "POST".into()),
            (
                format!("repos/{FIRST}/issues/comments/10/reactions"),
                "POST".into()
            ),
            (format!("repos/{FIRST}/issues/comments/10"), "PATCH".into()),
            (format!("repos/{FIRST}/issues/7/comments"), "POST".into()),
        ]
    );
    exact(&s, &f, &steps);
    assert_eq!(protected(&f), kept);
    assert_eq!(git_state(&f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&f);
    s.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn conversation_cards_v115_keyboard_add_comment_react_edit_and_quote_reply() {
    cards_journey(false);
}

#[test]
fn conversation_cards_v115_mouse_add_comment_react_edit_and_quote_reply() {
    cards_journey(true);
}
