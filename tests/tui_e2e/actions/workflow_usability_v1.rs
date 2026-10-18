//! Workflow page usability: one-key D/X/V controls with matching clickable
//! buttons, a readable definition summary and a plain-language dispatch preview.
use super::*;

fn steps() -> (Vec<GhStep>, usize, usize, usize) {
    let mut steps = initial();
    steps.extend([
        read("repos/example/repo", json!({"id":1,"full_name":"example/repo","default_branch":"main"}), false),
        read("repos/example/repo/actions/workflows?per_page=100",json!([{"workflows":[workflow("active")]}]),true),
        read("repos/example/repo/commits/main",json!({"sha":"a".repeat(40)}),false),
        GhStep::read(&["api","repos/example/repo/contents/.github/workflows/ci.yml?ref=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","--hostname","github.invalid","--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"], DISPATCH_YAML),
    ]);
    let mut dispatched = raw_run();
    dispatched["id"] = json!(200);
    dispatched["run_attempt"] = json!(1);
    dispatched["event"] = json!("workflow_dispatch");
    dispatched["head_branch"] = json!("main");
    dispatched["status"] = json!("queued");
    dispatched["conclusion"] = Value::Null;
    steps.extend(compose_identity_steps());
    let disable_cancelled = steps.len();
    steps.extend(compose_identity_steps());
    let dispatch_open = steps.len();
    steps.extend(current_account());
    steps.extend(workflow_guard("active"));
    steps.extend([
        read(
            "repos/example/repo/commits/main",
            json!({"sha":"a".repeat(40)}),
            false,
        ),
        write(
            "repos/example/repo/actions/workflows/7/dispatches",
            Some(json!({"ref":"main","inputs":{"debug":false,"mode":"fast"}})),
            json!({"workflow_run_id":200}),
        ),
        read("repos/example/repo/actions/runs/200", dispatched, false),
    ]);
    steps.extend(current_account());
    let dispatched_count = steps.len();
    (steps, disable_cancelled, dispatch_open, dispatched_count)
}

fn workflow_key(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        command(session, true, label);
    } else {
        session.send_key(key);
    }
}

#[test]
fn v1_actions_workflows_keyboard_d_x_keys_plain_preview_and_dispatch() {
    workflow_usability(false)
}
#[test]
fn v1_actions_workflows_mouse_buttons_plain_preview_and_dispatch() {
    workflow_usability(true)
}

fn workflow_usability(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let (steps, disable_cancelled, dispatch_open, dispatched_count) = steps();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123");
    choose(&mut session, mouse, "Workflows", b"W");
    session.wait_for_screen("Build dispatch • active • .github/workflows/ci.yml");
    session.wait_for_screen("Enter/click open workflow");
    journal(&mut session, &fixture, 5);
    if mouse {
        session.click_visible_action("Build dispatch");
    } else {
        session.send_key(b"\r");
    }
    // Readable definition summary and the page's own key legend.
    session.wait_for_screen("Manual runs: press D to dispatch on main with the 2 input(s) below");
    session.wait_for_screen("Input mode • one of safe, fast • optional • default safe");
    session.wait_for_screen("Definition job report • needs build");
    session.wait_for_screen("D dispatch • X enable/disable • V revision");
    session.wait_for_screen("[D Dispatch workflow]");
    session.wait_for_screen("[X Disable workflow]");
    journal(&mut session, &fixture, 7);

    // X opens Disable (the applicable toggle); preview is plain language; closing writes nothing.
    workflow_key(&mut session, mouse, "[X Disable workflow]", b"X");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("Workflow: Build dispatch (.github/workflows/ci.yml) in example/repo");
    session.wait_for_screen("GitHub stops starting new runs of this workflow");
    assert!(!session.screen().contains("Settings scope:"));
    close_form(&mut session, mouse);
    journal(&mut session, &fixture, disable_cancelled);

    // D opens Dispatch with the readable ref field; choose an input and preview.
    workflow_key(&mut session, mouse, "[D Dispatch workflow]", b"D");
    session.wait_for_screen("ACTIONS INTENT");
    session.wait_for_screen("Branch or tag to run");
    focus_field(&mut session, mouse, "mode — Build mode", 2);
    if mouse {
        session.click_visible_action("[fast]");
    } else {
        session.send_key(b" ");
    }
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("Branch or tag: main (commit aaaaaaaa");
    session.wait_for_screen("Input mode: fast");
    session.wait_for_screen("Input debug: false");
    assert!(!session.screen().contains("Exact affected target:"));
    journal(&mut session, &fixture, dispatch_open);
    if !mouse {
        session.send_key(b"\x1b[Z\x1b[Z");
    }
    confirm(&mut session, mouse, 3);
    session.wait_for_screen(
        "Workflow run #200 started on main (commit aaaaaaaa); press R to follow it",
    );
    let rows = journal(&mut session, &fixture, dispatched_count);
    let writes = rows
        .iter()
        .filter(|row| row["args"][5] == "POST" || row["args"][5] == "PUT")
        .collect::<Vec<_>>();
    assert_eq!(writes.len(), 1, "only the confirmed dispatch writes");
    assert_eq!(
        writes[0]["args"][1],
        "repos/example/repo/actions/workflows/7/dispatches"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("D dispatch • X enable/disable");
    session.quit_and_assert_restored_terminal(mouse);
}
