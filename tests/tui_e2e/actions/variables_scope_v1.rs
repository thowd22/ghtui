//! TASK-59: a repository variable is created and then deleted through confirmed,
//! read-back writes, and the environment scope lists and creates its own variables.
//! Keyboard and mouse journeys with strict offline fixtures; no Git changes.
use super::*;

fn variable(name: &str, value: &str) -> Value {
    json!({"name":name,"value":value,"created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:00:00Z"})
}
fn list(prefix: &str, variables: Vec<Value>) -> GhStep {
    read(
        &format!("{prefix}/variables?per_page=100"),
        json!([{ "variables": variables }]),
        true,
    )
}
fn guarded() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
    ]
}
/// Open, preview and confirm a create: absence check, POST, exact read-back.
fn create(steps: &mut Vec<GhStep>, prefix: &str, existing: Vec<Value>, name: &str, value: &str) {
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(guarded());
    steps.push(list(prefix, existing));
    steps.push(write(
        &format!("{prefix}/variables"),
        Some(json!({"name":name,"value":value})),
        json!({}),
    ));
    steps.push(read(
        &format!("{prefix}/variables/{name}"),
        variable(name, value),
        false,
    ));
    steps.extend(current_account());
}
fn set_form(session: &mut Session, mouse: bool, name: &str, value: &str) {
    command(session, mouse, "Set variable");
    session.wait_for_screen("ACTIONS INTENT");
    session.send_key(b"\x15");
    session.paste(name);
    focus_field(session, mouse, "Variable value", 1);
    session.send_key(b"\x15");
    session.paste(value);
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    if !mouse {
        session.send_key(b"\x1b[Z");
    }
    confirm(session, mouse, 2);
}

#[test]
fn actions_secrets_vars_keyboard_variable_create_delete_and_environment_scope() {
    variables(false)
}
#[test]
fn actions_secrets_vars_mouse_variable_create_delete_and_environment_scope() {
    variables(true)
}

fn variables(mouse: bool) {
    const REPO: &str = "repos/example/repo/actions";
    const ENV: &str = "repos/example/repo/environments/production";
    let fixture = fixture();
    let mut steps = initial();
    steps.push(list(REPO, vec![variable("MODE", "east")]));
    let listed = steps.len();
    create(
        &mut steps,
        REPO,
        vec![variable("MODE", "east")],
        "STAGE",
        "blue",
    );
    let created = steps.len();
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.extend(guarded());
    steps.push(read(
        &format!("{REPO}/variables/STAGE"),
        variable("STAGE", "blue"),
        false,
    ));
    let mut deletion = write(&format!("{REPO}/variables/STAGE"), None, Value::Null);
    deletion.args[5] = "DELETE".into();
    steps.push(deletion);
    steps.push(list(REPO, vec![variable("MODE", "east")]));
    steps.extend(current_account());
    let deleted = steps.len();
    // Environment scope: its secrets page loads first, then its own variables.
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.push(read(
        &format!("{ENV}/secrets?per_page=100"),
        json!([{"secrets":[{"name":"PROD_TOKEN","created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:00:00Z"}]}]),
        true,
    ));
    steps.extend(current_account());
    let scoped = steps.len();
    steps.push(list(ENV, vec![variable("REGION", "eu")]));
    let env_listed = steps.len();
    create(
        &mut steps,
        ENV,
        vec![variable("REGION", "eu")],
        "TIER",
        "gold",
    );
    let env_created = steps.len();
    fixture.scripted_gh(&steps);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    choose(&mut session, mouse, "Variables", b"v");
    session.wait_for_screen("MODE = east");
    journal(&mut session, &fixture, listed);

    set_form(&mut session, mouse, "STAGE", "blue");
    session.wait_for_screen("Variable STAGE value confirmed");
    session.wait_for_screen("STAGE = blue");
    journal(&mut session, &fixture, created);

    command(&mut session, mouse, "Delete variable");
    session.wait_for_screen("ACTIONS INTENT");
    session.send_key(b"\x15");
    session.paste("STAGE");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("Variable STAGE deletion confirmed");
    session.wait_for_screen("MODE = east");
    assert!(!session.screen().contains("STAGE = blue"));
    journal(&mut session, &fixture, deleted);

    // Choose the environment scope by key/click: kind, then its name.
    command(&mut session, mouse, "Choose Actions scope");
    session.wait_for_screen("ACTIONS INTENT");
    if mouse {
        session.click_visible_action("[environment]");
    } else {
        session.send_key(b" ");
    }
    focus_field(&mut session, mouse, "Environment or organization name", 1);
    session.paste("production");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    if !mouse {
        session.send_key(b"\x1b[Z");
    }
    confirm(&mut session, mouse, 2);
    session.wait_for_screen("PROD_TOKEN");
    journal(&mut session, &fixture, scoped);
    choose(&mut session, mouse, "Variables", b"v");
    session.wait_for_screen("REGION = eu");
    assert!(!session.screen().contains("MODE = east"));
    journal(&mut session, &fixture, env_listed);
    set_form(&mut session, mouse, "TIER", "gold");
    session.wait_for_screen("Variable TIER value confirmed");
    session.wait_for_screen("TIER = gold");
    let rows = journal(&mut session, &fixture, env_created);

    let writes = rows
        .iter()
        .filter(|row| {
            matches!(
                row["args"][5].as_str(),
                Some("POST" | "PATCH" | "PUT" | "DELETE")
            )
        })
        .map(|row| {
            format!(
                "{} {}",
                row["args"][5].as_str().unwrap(),
                row["args"][1].as_str().unwrap()
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        writes,
        vec![
            format!("POST {REPO}/variables"),
            format!("DELETE {REPO}/variables/STAGE"),
            format!("POST {ENV}/variables"),
        ]
    );
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 3);
    assert!(durable.iter().all(|row| row.state == "confirmed"));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("TIER = gold");
    session.quit_and_assert_restored_terminal(mouse);
}
