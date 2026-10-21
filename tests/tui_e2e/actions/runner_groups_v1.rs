//! TASK-60: in an explicitly chosen organization scope, runner groups are listed, their
//! repository and runner associations are replaced through a confirmed PUT plus exact
//! read-back, and an idle self-hosted runner is removed through a confirmed DELETE plus a
//! read-back that no longer lists it. Keyboard and mouse journeys with strict offline
//! fixtures; no Git changes.
use super::*;

const ORG: &str = "orgs/example/actions";

fn put(endpoint: &str, input: Value) -> GhStep {
    let mut request = write(endpoint, Some(input), Value::Null);
    request.args[5] = "PUT".into();
    request
}
fn group() -> Value {
    json!({"id":7,"name":"builders","visibility":"selected","default":false,"inherited":false,"allows_public_repositories":false,"restricted_to_workflows":false,"selected_workflows":[],"workflow_restrictions_read_only":false})
}
fn members(key: &str, ids: &[u64]) -> GhStep {
    read(
        &format!("{ORG}/runner-groups/7/{key}?per_page=100"),
        json!([{ key: ids.iter().map(|id| json!({"id":id})).collect::<Vec<_>>() }]),
        true,
    )
}
/// Preview reads the observed members; Confirm re-checks the principal, the exact group and
/// its members, replaces the complete set, and reads it back.
fn replace(steps: &mut Vec<GhStep>, key: &str, payload: &str, observed: &[u64], new: &[u64]) {
    steps.extend(compose_identity_steps());
    steps.push(members(key, observed));
    steps.extend(current_account());
    steps.extend(current_account());
    steps.push(read(&format!("{ORG}/runner-groups/7"), group(), false));
    steps.push(members(key, observed));
    steps.push(put(
        &format!("{ORG}/runner-groups/7/{key}"),
        json!({ payload: new }),
    ));
    steps.push(members(key, new));
    steps.extend(current_account());
}
fn confirm_preview(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
}
fn next_field(session: &mut Session, mouse: bool, label: &str) {
    let previous_row = session
        .screen()
        .lines()
        .position(|line| line.contains(label));
    choose(session, mouse, "[F6 Fields↓]", b"\x1b[17~");
    session
        .try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .position(|line| line.contains(label))
                    .is_some_and(|row| Some(row) != previous_row)
            },
            &format!("Field viewport moved to {label}"),
            std::time::Duration::from_secs(8),
        )
        .unwrap();
}
fn replace_form(session: &mut Session, mouse: bool, label: &str, ids: &str) {
    command(session, mouse, label);
    session.wait_for_screen("ACTIONS INTENT");
    session.wait_for_screen("Exact selected ID");
    next_field(session, mouse, "New complete set of IDs, comma-separated");
    session.send_key(b"\x15");
    session.paste(ids);
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
}
fn select_row(session: &mut Session, mouse: bool, label: &str) {
    session.wait_for_screen(label);
    if mouse {
        session.click_visible_action(label);
        wait_selected(session, label);
        return;
    }
    session.send_key(b"\x1b[H");
    for _ in 0..20 {
        session.poll_output(std::time::Duration::from_millis(40));
        if selected_has(session, label) {
            return;
        }
        session.send_key(b"\x1b[B");
    }
    panic!("Exact visible row {label} did not gain keyboard selection");
}
fn stdin_json(row: &Value) -> Value {
    let hex = row["stdin_hex"].as_str().unwrap();
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn actions_settings_keyboard_organization_runner_group_associations_and_confirmed_runner_removal() {
    runner_groups(false)
}
#[test]
fn actions_settings_mouse_organization_runner_group_associations_and_confirmed_runner_removal() {
    runner_groups(true)
}

fn runner_groups(mouse: bool) {
    let fixture = fixture();
    let unchanged = fixture.git(&["rev-parse", "HEAD"]);
    let idle = json!({"id":9,"name":"builder","os":"linux","status":"online","busy":false,"labels":[{"id":1,"name":"self-hosted","type":"read-only"}]});
    let busy = json!({"id":10,"name":"overloaded","os":"linux","status":"online","busy":true,"labels":[{"id":1,"name":"self-hosted","type":"read-only"}]});
    let mut steps = initial();
    // Choose scope: form OPEN, local-only confirmation, the organization's secrets page.
    steps.extend(compose_identity_steps());
    steps.extend(current_account());
    steps.push(read(
        &format!("{ORG}/secrets?per_page=100"),
        json!([{"secrets":[]}]),
        true,
    ));
    steps.extend(current_account());
    let scoped = steps.len();
    steps.push(read(
        &format!("{ORG}/runner-groups?per_page=100"),
        json!([{"runner_groups":[group()]}]),
        true,
    ));
    let groups_listed = steps.len();
    replace(
        &mut steps,
        "repositories",
        "selected_repository_ids",
        &[9],
        &[10, 11],
    );
    let repositories_previewed = groups_listed + 5;
    let repositories_replaced = steps.len();
    replace(&mut steps, "runners", "runners", &[9], &[9, 12, 13]);
    let runners_previewed = repositories_replaced + 5;
    let runners_replaced = steps.len();
    steps.push(read(
        &format!("{ORG}/runners?per_page=100"),
        json!([{"runners":[idle.clone(),busy.clone()]}]),
        true,
    ));
    let runners_listed = steps.len();
    steps.extend(compose_identity_steps());
    let removal_previewed = steps.len();
    steps.extend(current_account());
    steps.extend(current_account());
    steps.push(read(&format!("{ORG}/runners/9"), idle, false));
    let mut removal = write(&format!("{ORG}/runners/9"), None, Value::Null);
    removal.args[5] = "DELETE".into();
    steps.push(removal);
    steps.push(read(
        &format!("{ORG}/runners?per_page=100"),
        json!([{"runners":[busy]}]),
        true,
    ));
    steps.extend(current_account());
    let removed = steps.len();
    fixture.scripted_gh(&steps);

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    // Pick the organization scope by key/click: kind (third choice), then its name.
    command(&mut session, mouse, "Choose Actions scope");
    session.wait_for_screen("ACTIONS INTENT");
    if mouse {
        session.click_visible_action("[organization]");
    } else {
        session.send_key(b"  ");
    }
    focus_field(&mut session, mouse, "Environment or organization name", 1);
    session.paste("example");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Actions settings scope selected");
    journal(&mut session, &fixture, scoped);

    choose(&mut session, mouse, "Runner groups", b"g");
    session.wait_for_screen("#7 • builders • repositories selected");
    session.wait_for_screen("public repositories blocked • workflows all workflows");
    journal(&mut session, &fixture, groups_listed);

    replace_form(&mut session, mouse, "Replace group repositories", "10,11");
    journal(&mut session, &fixture, repositories_previewed);
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Runner group 7 association set confirmed: 2 entries");
    journal(&mut session, &fixture, repositories_replaced);

    replace_form(&mut session, mouse, "Replace group runners", "9,12,13");
    journal(&mut session, &fixture, runners_previewed);
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Runner group 7 association set confirmed: 3 entries");
    journal(&mut session, &fixture, runners_replaced);

    choose(&mut session, mouse, "Runners", b"n");
    session.wait_for_screen("#10 • overloaded");
    journal(&mut session, &fixture, runners_listed);
    select_row(&mut session, mouse, "#9 • builder");
    command(&mut session, mouse, "Remove selected runner");
    session.wait_for_screen("ACTIONS INTENT");
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    journal(&mut session, &fixture, removal_previewed);
    confirm_preview(&mut session, mouse);
    session.wait_for_screen("Runner 9 removed; no service installed or compute provisioned");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("#9 • builder") && screen.contains("#10 • overloaded"),
            "removed runner left the read-back inventory",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
    let rows = journal(&mut session, &fixture, removed);

    let writes = rows
        .iter()
        .filter(|row| matches!(row["args"][5].as_str(), Some("PUT" | "DELETE" | "POST")))
        .collect::<Vec<_>>();
    assert_eq!(
        writes
            .iter()
            .map(|row| format!(
                "{} {}",
                row["args"][5].as_str().unwrap(),
                row["args"][1].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        vec![
            format!("PUT {ORG}/runner-groups/7/repositories"),
            format!("PUT {ORG}/runner-groups/7/runners"),
            format!("DELETE {ORG}/runners/9"),
        ]
    );
    assert_eq!(
        stdin_json(writes[0]),
        json!({"selected_repository_ids":[10,11]})
    );
    assert_eq!(stdin_json(writes[1]), json!({"runners":[9,12,13]}));
    assert_eq!(writes[2]["stdin_hex"], "");
    let durable =
        publication_support::durable_rows(&fixture.root.join("config/ghtui/cache.sqlite3"));
    assert_eq!(durable.len(), 3);
    assert!(durable
        .iter()
        .all(|row| row.state == "confirmed" && row.principal == "user-id:42"));
    assert_eq!(durable[0].kind, json!("group_repositories"));
    assert_eq!(durable[1].kind, json!("group_runners"));
    assert_eq!(durable[2].kind, json!("remove_runner"));
    for row in &durable[..2] {
        assert_eq!(row.target["GroupAssociations"]["organization"], "example");
        assert_eq!(row.target["GroupAssociations"]["id"], 7);
    }
    assert_eq!(durable[2].target["Runner"]["id"], 9);
    assert_eq!(
        durable[2].target["Runner"]["scope"],
        json!({"Organization":{"organization":"example"}})
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), unchanged);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("#10 • overloaded");
    session.quit_and_assert_restored_terminal(mouse);
}
