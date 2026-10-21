//! TASK-64 AC3: the PR Summary's review decisions draw reviewers through the shared identity
//! fallback (a bot keeps `[bot]`, a deleted or missing reviewer is `unknown actor`, never a fake
//! `@unknown` account), and PR Checks colour every check/status state through the one shared
//! CI mapping: an errored commit status fails (red ✗) as it does in Inbox and Actions, and a
//! skipped check run is muted (○), not waiting (…). Keyboard and mouse, read-only.
use super::*;
use ghtui::theme::{SemanticToken, Theme, CATPPUCCIN_MOCHA};

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const REVIEWS_LINE: &str = "Reviews: @ci-helper [bot] approved • unknown actor [unknown] commented";

fn reviews() -> Value {
    json!([[
        {"id":901,"node_id":"PRR_901","state":"APPROVED","body":"","commit_id":HEAD,
         "user":{"id":77,"login":"ci-helper","type":"Bot"},"submitted_at":"2026-10-17T00:00:00Z"},
        {"id":902,"node_id":"PRR_902","state":"COMMENTED","body":"Left before deletion","commit_id":HEAD,
         "user":null,"submitted_at":"2026-10-17T01:00:00Z"}
    ]])
}

fn steps() -> Vec<GhStep> {
    let mut steps = initial();
    let mut opened = details(vec![starter()]);
    for read in &mut opened {
        if read.args.get(1).map(String::as_str)
            == Some("repos/example/repo/pulls/7/reviews?per_page=100")
        {
            read.stdout = serde_json::to_vec(&reviews()).unwrap();
        }
    }
    steps.extend(opened);
    steps.push(step(
        &format!("repos/example/repo/commits/{HEAD}/check-runs?per_page=100"),
        "GET",
        json!([{"check_runs":[
            {"id":61,"name":"Unit tests","status":"completed","conclusion":"success","head_sha":HEAD},
            {"id":62,"name":"Lint docs","status":"completed","conclusion":"skipped","head_sha":HEAD}
        ]}]),
        true,
    ));
    steps.push(step(
        &format!("repos/example/repo/commits/{HEAD}/statuses?per_page=100"),
        "GET",
        json!([[{"id":63,"context":"ci/legacy","state":"error",
                 "target_url":"https://ci.invalid/legacy/63"}]]),
        true,
    ));
    steps.push(step(
        "repos/example/repo/branches/main",
        "GET",
        json!({"name":"main","protected":false}),
        false,
    ));
    steps.push(step(
        "repos/example/repo/rules/branches/main?per_page=100",
        "GET",
        json!([[]]),
        true,
    ));
    steps
}

/// The first cell of `text` on the screen line that contains `row`.
fn glyph_cell<'a>(screen: &'a vt100::Screen, row: &str, text: &str) -> Option<&'a vt100::Cell> {
    let (rows, cols) = screen.size();
    (0..rows).find_map(|y| {
        let line: String = (0..cols)
            .map(|x| screen.cell(y, x).map_or(" ".into(), |c| c.contents()))
            .map(|c| if c.is_empty() { " ".to_owned() } else { c })
            .collect();
        if !line.contains(row) {
            return None;
        }
        (0..cols).find_map(|x| screen.cell(y, x).filter(|cell| cell.contents() == text))
    })
}

/// The default theme's truecolor ink for a semantic token.
fn ink(token: SemanticToken) -> vt100::Color {
    match Theme::from_settings(0, false).style(token).fg {
        Some(ratatui::style::Color::Rgb(r, g, b)) => vt100::Color::Rgb(r, g, b),
        other => panic!("{token:?} has no truecolor ink: {other:?}"),
    }
}

fn journey(mouse: bool) {
    let fixture = configured();
    let git = local_git_witness(&fixture);
    let steps = steps();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Native collaboration");
    choose(&mut session, mouse, "Native collaboration", b"\r");
    session.wait_for_screen("1 comments");
    // Reviewers read as the shared identity label says.
    session.wait_for_screen(REVIEWS_LINE);
    assert!(
        !session.screen().contains("@unknown"),
        "{}",
        session.screen()
    );
    records(&mut session, &fixture, steps.len() - 4);
    // Checks: one read of each source; every state takes the shared token.
    let after = session.completed_cells_end();
    choose(&mut session, mouse, "[k Checks]", b"k");
    let (red, muted, green) = (
        ink(SemanticToken::Failure),
        ink(SemanticToken::Muted),
        ink(SemanticToken::Success),
    );
    // The Failure ink is the palette's red text, distinct from waiting/muted grey.
    assert_eq!(
        red,
        vt100::Color::Rgb(0xf3, 0x8b, 0xa8),
        "{:?}",
        CATPPUCCIN_MOCHA.red_text
    );
    assert_ne!(red, ink(SemanticToken::Pending));
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        let settled = session.completed_cells_end() > after
            && session.completed_cells_screen().is_some_and(|frame| {
                let failed = glyph_cell(frame, "ci/legacy", "✗");
                let skipped = glyph_cell(frame, "Lint docs", "○");
                let passed = glyph_cell(frame, "Unit tests", "✓");
                failed.is_some_and(|cell| cell.fgcolor() == red)
                    && skipped.is_some_and(|cell| cell.fgcolor() == muted)
                    && passed.is_some_and(|cell| cell.fgcolor() == green)
            });
        if settled {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Checks rows did not take the shared CI tokens\n{}",
            session.screen()
        );
    }
    let screen = session.screen();
    for (row, state) in [("ci/legacy", "error"), ("Lint docs", "skipped")] {
        let line = screen
            .lines()
            .find(|line| line.contains(row))
            .unwrap_or_else(|| panic!("{row} row\n{screen}"));
        assert!(line.contains(state), "{line}");
        assert!(
            !line.contains('…'),
            "a concluded state is never waiting: {line}"
        );
    }
    // Read-only: exactly the scripted reads, no write, the checkout untouched.
    let journal = records(&mut session, &fixture, steps.len());
    assert_eq!(mutation_count(&journal), 0);
    assert_eq!(local_git_witness(&fixture), git, "local checkout untouched");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn audit_v64_keyboard_reviews_and_checks_follow_shared_identity_and_ci_tokens() {
    journey(false)
}
#[test]
fn audit_v64_mouse_reviews_and_checks_follow_shared_identity_and_ci_tokens() {
    journey(true)
}
