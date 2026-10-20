//! TASK-54: `V Choose workflow revision` re-reads the definition at another ref by key and
//! by click, shows that revision's exact commit and raw YAML, and a revision without
//! `workflow_dispatch` explains why Dispatch is unavailable. Nothing is written.
use super::*;

const PUSH_ONLY_YAML: &str = "name: Build dispatch\non:\n  push:\n    branches: [feature/fix]\njobs:\n  lint:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo revision-only-step\n";

fn steps() -> (Vec<GhStep>, usize, usize, usize) {
    let mut steps = initial();
    steps.extend([
        read("repos/example/repo", json!({"id":1,"full_name":"example/repo","default_branch":"main"}), false),
        read("repos/example/repo/actions/workflows?per_page=100",json!([{"workflows":[workflow("active")]}]),true),
        read("repos/example/repo/commits/main",json!({"sha":"a".repeat(40)}),false),
        GhStep::read(&["api","repos/example/repo/contents/.github/workflows/ci.yml?ref=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","--hostname","github.invalid","--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"], DISPATCH_YAML),
    ]);
    let definition = steps.len();
    steps.extend(compose_identity_steps());
    let opened = steps.len();
    steps.extend(current_account());
    steps.extend([
        read("repos/example/repo/commits/feature%2Ffix",json!({"sha":"b".repeat(40)}),false),
        GhStep::read(&["api","repos/example/repo/contents/.github/workflows/ci.yml?ref=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","--hostname","github.invalid","--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"], PUSH_ONLY_YAML),
    ]);
    steps.extend(current_account());
    let inspected = steps.len();
    (steps, definition, opened, inspected)
}

#[test]
fn v1_actions_workflow_revision_keyboard_reads_other_ref_and_explains_no_dispatch() {
    revision(false)
}
#[test]
fn v1_actions_workflow_revision_mouse_reads_other_ref_and_explains_no_dispatch() {
    revision(true)
}

fn revision(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let (steps, definition, opened, inspected) = steps();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    choose(&mut session, mouse, "Workflows", b"W");
    wait_row(
        &mut session,
        &["Build dispatch", "active", ".github/workflows/ci.yml"],
    );
    if mouse {
        session.click_visible_action("Build dispatch");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("revision main • exact commit aaaaaaaa");
    session.wait_for_control("[D Dispatch workflow]");
    journal(&mut session, &fixture, definition);

    // V (or its button) opens the revision form prefilled with the current revision.
    if mouse {
        command(&mut session, true, "[V Choose workflow revision]");
    } else {
        session.send_key(b"V");
    }
    session.wait_for_screen("ACTIONS INTENT");
    session.wait_for_screen("Inspect workflow at branch, tag or commit");
    session.send_key(b"\x15");
    session.paste("feature/fix");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("revision: feature/fix");
    journal(&mut session, &fixture, opened);
    confirm(&mut session, mouse, 1);

    // The definition now reflects the chosen ref, its exact commit and its raw YAML.
    session.wait_for_screen("revision feature/fix • exact commit bbbbbbbb");
    session.wait_for_screen("Triggers: push");
    session.wait_for_screen("Manual runs: not available (no workflow_dispatch trigger)");
    session.wait_for_screen("revision-only-step");
    session.assert_control_absent("[D Dispatch workflow]");
    session.wait_for_control("[V Choose workflow revision]");
    let rows = journal(&mut session, &fixture, inspected);

    assert!(rows
        .iter()
        .all(|row| row["args"].as_array().is_some_and(|args| !args
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))))));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_control("[V Revision]");
    // D's key explains why manual runs are unavailable at this revision; nothing is sent.
    session.send_key(b"D");
    let body = session.wait_for_overlay_body("Action unavailable");
    let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        body.contains("cannot be started by hand"),
        "explanation names the missing trigger: {body}"
    );
    journal(&mut session, &fixture, inspected);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("Action unavailable"),
            "explanation closed",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    session.quit_and_assert_restored_terminal(mouse);
}
