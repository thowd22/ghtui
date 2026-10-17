//! TASK54W7 typed native inputs use the same actual schema and publisher.
//! SOURCE ONLY/UNRUN: no handler invocation or fabricated schema/source.
use super::*;
const TYPED: &str = "name: PRIVATE_DISPATCH\non:\n  workflow_dispatch:\n    inputs:\n      message:\n        type: string\n        required: true\n      debug:\n        type: boolean\n        required: true\n      count:\n        type: number\n        required: true\n      channel:\n        type: choice\n        required: true\n        options: [stable, beta]\n      deploy:\n        type: environment\n        required: true\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_RUN_LOCALLY\n";
fn environment(p: Protocol) -> GhStep {
    get_versioned(
        &format!("repos/{CURRENT}/environments/release-env"),
        json!({"id":71,"name":"release-env"}),
        p,
    )
}
fn phase(p: Protocol, preview: bool) -> Vec<GhStep> {
    let mut steps = schema_phase(p);
    steps[8].stdout = TYPED.as_bytes().to_vec();
    if preview {
        steps.insert(9, environment(p));
    }
    assert_eq!(steps.len(), if preview { 16 } else { 15 });
    steps
}
fn confirmation() -> Vec<GhStep> {
    let p = Protocol::Detailed;
    let base = super::confirmation(p, 42);
    let mut steps = base[..9].to_vec();
    steps[8].stdout = TYPED.as_bytes().to_vec();
    steps.push(environment(p));
    steps.extend_from_slice(&base[9..14]);
    steps.push(environment(p));
    steps.extend_from_slice(&base[14..]);
    let post = steps
        .iter_mut()
        .find(|s| s.args.iter().any(|a| a == "POST"))
        .unwrap();
    post.stdin = serde_json::to_vec(&json!({"ref":"main","inputs":{
        "message":SECRET,"debug":true,"count":7,"channel":"beta","deploy":"release-env"
    }}))
    .unwrap();
    assert_eq!(steps.len(), 23);
    steps
}
fn paste(s: &mut Session, mouse: bool, name: &str, value: &str) {
    field(
        s,
        mouse,
        &format!("{name} • transient input value • masked; never saved"),
    );
    let after = s.completed_cells_end();
    s.paste(value);
    owned(s, after, INTENT, &["EXACT DISPATCH SCHEMA", "[Preview]"]);
}
fn choice(s: &mut Session, mouse: bool, name: &str, value: &str) {
    field(
        s,
        mouse,
        &format!("{name} • transient input value • masked; never saved"),
    );
    let after = s.completed_cells_end();
    if mouse {
        let (x, y) = s.wait_for_owned_completed_action(INTENT, &format!("[{value}]"), &[]);
        s.click(x, y);
    } else {
        // Explicit exact value through the shared secret input editor. The
        // parallel mouse case uses the actually rendered indexed choice.
        s.paste(value);
    }
    owned(s, after, INTENT, &["EXACT DISPATCH SCHEMA", "[Preview]"]);
}
fn typed(mouse: bool) {
    let p = Protocol::Detailed;
    let mut expected = opening();
    expected.extend(phase(p, false));
    expected.extend(phase(p, true));
    expected.extend(confirmation());
    let f = configured_native(&expected);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = start(&f, mouse);
    begin(&mut s, &f, mouse, p);
    let protected = unrelated(&f);
    inspect_schema(&mut s, &f, mouse);
    for (name, kind) in [
        ("message", "text"),
        ("debug", "true/false"),
        ("count", "number"),
        ("channel", "one option"),
        ("deploy", "environment"),
    ] {
        field(&mut s, mouse, &format!("{name} • {kind} / required • mode"));
        let shown =
            crate::support::overlay_body(s.completed_cells_screen().unwrap(), INTENT).unwrap();
        assert!(!shown.contains("Some(") && !shown.contains("numeric UID"));
    }
    paste(&mut s, mouse, "message", SECRET);
    choice(&mut s, mouse, "debug", "true");
    paste(&mut s, mouse, "count", "7");
    choice(&mut s, mouse, "channel", "beta");
    paste(&mut s, mouse, "deploy", "release-env");
    for (width, height) in [(80, 20), (80, 24), (120, 40)] {
        resize_form(
            &mut s,
            width,
            height,
            INTENT,
            &["[Preview]", "[Confirm action]", "[Keep draft / Close]"],
        );
        let frame = s.completed_cells_screen().unwrap();
        let body = crate::support::overlay_body(frame, INTENT).unwrap();
        assert!(!body.contains(SECRET) && !body.contains("release-env"));
        assert_eq!(frame.size(), (height, width));
    }
    let after = s.completed_cells_end();
    action(&mut s, mouse, INTENT, "[Preview]");
    s.wait_for_command_count(&f, 70);
    owned(
        &mut s,
        after,
        INTENT,
        &["DISPATCH PREVIEW", "[Confirm action]", C],
    );
    let body = crate::support::overlay_body(s.completed_cells_screen().unwrap(), INTENT).unwrap();
    assert!(!body.contains(SECRET) && !body.contains("release-env"));
    let tty = s.actual_terminal_witness_v4().unwrap();
    let after = s.completed_cells_end();
    action(&mut s, mouse, INTENT, "[Confirm action]");
    s.wait_for_command_count(&f, expected.len());
    owned(
        &mut s,
        after,
        HISTORY,
        &[
            "[F11 Open run]",
            "[F6 Acknowledge locally]",
            "[Close]",
            "Accepted; completion unknown",
            "[F7 Info↑]",
            "[F8 Info↓]",
        ],
    );
    let metadata = metadata(&f);
    assert_eq!(metadata.0, "accepted_pending");
    for text in [&metadata.1, &metadata.2, &metadata.3] {
        assert!(
            !text.contains(SECRET)
                && !text.contains("release-env")
                && !text.contains("NEVER_RUN_LOCALLY")
        );
    }
    assert_eq!(git_state(&f), git);
    assert_eq!(unrelated(&f), protected);
    assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
    s.capture_completed_terminal_v1("task54-w7-typed-boolean-number-choice-environment-receipt");
    close(&mut s, mouse, HISTORY);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &expected, 1);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_private_intents_unchanged(&f);
    assert_read_only_git(&f);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_keyboard_typed_boolean_number_choice_environment_masked_and_exact_post(
) {
    typed(false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_mouse_typed_boolean_number_choice_environment_masked_and_exact_post(
) {
    typed(true);
}
