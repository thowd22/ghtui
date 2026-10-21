//! TASK-59: in an explicitly chosen environment scope, secret metadata is listed, a synthetic
//! secret is set through `gh secret set --env` with masked transient stdin, and it is then
//! deleted through a confirmed DELETE whose read-back no longer lists it. Keyboard and mouse
//! journeys with strict offline fixtures; the value never reaches the screen, journal or
//! product files, and Git is unchanged.
use super::*;

const ENV: &str = "repos/example/repo/environments/production";
const SECRET: &str = "synthetic-environment-secret-never-persist";

fn secret(name: &str) -> Value {
    json!({"name":name,"created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:00:00Z"})
}
fn listing(names: &[&str]) -> GhStep {
    read(
        &format!("{ENV}/secrets?per_page=100"),
        json!([{ "secrets": names.iter().map(|name| secret(name)).collect::<Vec<_>>() }]),
        true,
    )
}
fn confirm_preview(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
}

#[test]
fn actions_secrets_vars_keyboard_environment_scope_secret_set_and_confirmed_delete() {
    environment_secrets(false)
}
#[test]
fn actions_secrets_vars_mouse_environment_scope_secret_set_and_confirmed_delete() {
    environment_secrets(true)
}

fn environment_secrets(mouse: bool) {
    let fixture = fixture();
    let unchanged = fixture.git(&["rev-parse", "HEAD"]);
    let mut steps = initial();
    // Choose scope: form OPEN, local-only confirmation, then the environment's secrets.
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.push(listing(&["PROD_TOKEN"]));
    steps.extend(current_account());
    let scoped = steps.len();
    steps.extend(compose_identity_steps());
    let set_previewed = steps.len();
    steps.extend(current_account());
    steps.extend(current_account());
    steps.push(listing(&["PROD_TOKEN"]));
    let mut set = GhStep::read(
        &[
            "secret",
            "set",
            "STAGING_KEY",
            "--app",
            "actions",
            "--repo",
            "github.invalid/example/repo",
            "--env",
            "production",
        ],
        "",
    );
    set.stdin = SECRET.as_bytes().to_vec();
    steps.push(set);
    steps.push(listing(&["PROD_TOKEN", "STAGING_KEY"]));
    steps.extend(current_account());
    let set_confirmed = steps.len();
    steps.extend(compose_identity_steps());
    let delete_previewed = steps.len();
    steps.extend(current_account());
    steps.extend(current_account());
    steps.push(listing(&["PROD_TOKEN", "STAGING_KEY"]));
    let mut deletion = write(&format!("{ENV}/secrets/STAGING_KEY"), None, Value::Null);
    deletion.args[5] = "DELETE".into();
    steps.push(deletion);
    steps.push(listing(&["PROD_TOKEN"]));
    steps.extend(current_account());
    let deleted = steps.len();
    fixture.scripted_gh(&steps);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
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
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("PROD_TOKEN • updated 2026-10-13T12:00:00Z");
    session.wait_for_screen("Existing secret values cannot be read");
    journal(&mut session, &fixture, scoped);

    command(&mut session, mouse, "Set secret");
    session.wait_for_screen("ACTIONS INTENT");
    session.send_key(b"\x15");
    session.paste("STAGING_KEY");
    focus_field(&mut session, mouse, "New secret value", 1);
    session.paste(SECRET);
    assert!(!session.screen().contains(SECRET));
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    assert!(!session.screen().contains(SECRET));
    journal(&mut session, &fixture, set_previewed);
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Secret metadata confirmed");
    session.wait_for_screen("STAGING_KEY • updated 2026-10-13T12:00:00Z");
    journal(&mut session, &fixture, set_confirmed);

    command(&mut session, mouse, "Delete secret");
    session.wait_for_screen("ACTIONS INTENT");
    session.send_key(b"\x15");
    session.paste("STAGING_KEY");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    journal(&mut session, &fixture, delete_previewed);
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Secret STAGING_KEY deletion confirmed");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("STAGING_KEY • updated") && screen.contains("PROD_TOKEN"),
            "deleted environment secret left the read-back metadata",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    let rows = journal(&mut session, &fixture, deleted);

    let secret_writes = rows
        .iter()
        .filter(|row| row["args"][0] == "secret")
        .collect::<Vec<_>>();
    assert_eq!(secret_writes.len(), 1);
    assert_eq!(secret_writes[0]["stdin_hex"], "[REDACTED]");
    let deletes = rows
        .iter()
        .filter(|row| row["args"][5] == "DELETE")
        .map(|row| row["args"][1].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(deletes, vec![format!("{ENV}/secrets/STAGING_KEY")]);
    assert!(!rows
        .iter()
        .any(|row| matches!(row["args"][5].as_str(), Some("PUT" | "PATCH" | "POST"))));
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 2);
    assert!(durable
        .iter()
        .all(|row| row.state == "confirmed" && row.principal == "user-id:42"));
    assert_eq!(durable[0].kind, json!("set_secret"));
    assert_eq!(durable[1].kind, json!("delete_secret"));
    for row in &durable {
        assert_eq!(row.target["Secret"]["name"], "STAGING_KEY");
        assert_eq!(
            row.target["Secret"]["scope"],
            json!({"Environment":{"repository":"example/repo","environment":"production"}})
        );
    }
    assert!(!session.inspect_command_journal(&fixture).contains(SECRET));
    assert!(!String::from_utf8_lossy(&session.transcript).contains(SECRET));
    for path in product_files(&fixture.root.join("config")) {
        assert!(
            !std::fs::read(&path)
                .unwrap()
                .windows(SECRET.len())
                .any(|window| window == SECRET.as_bytes()),
            "secret persisted at {}",
            path.display()
        );
    }
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("PROD_TOKEN");
    session.quit_and_assert_restored_terminal(mouse);
}
