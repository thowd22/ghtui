//! Real explicit Close -> typed durable effect -> Back -> complete source
//! Refresh (and repeat). Both genuine origin families, same binary/actions,
//! independent exact argv/stdin/raw SQL/Git/config/TTY. No App injection.
use super::*;
use rusqlite::types::Value as S;
#[path = "pr_lifecycle_confirm_sql_v86.rs"]
mod confirm_sql_v86;
#[path = "pr_cross_family_state_issue_v106.rs"]
mod cross_family_v106;
fn t(value: &str) -> S {
    S::Text(value.to_owned())
}
fn fresh_cells(
    s: &mut Session,
    after: usize,
    mut condition: impl FnMut(&vt100::Screen) -> bool,
    label: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after && s.completed_cells_screen().is_some_and(&mut condition)
        {
            return;
        }
        if Instant::now() >= deadline {
            s.save_artifacts();
            panic!("{label}\n{}", s.screen());
        }
    }
}
fn confirmed_tail() -> Vec<GhStep> {
    let mut calls = state_phase(json!(true));
    let mut node = json!({"id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/scoped-open","baseRefName":"main","state":"CLOSED","isDraft":false,"merged":false,"repository":{"id":"REPO_catalogue_10000","nameWithOwner":FIRST}});
    let mutation="mutation($id:ID!){closePullRequest(input:{pullRequestId:$id}){pullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged repository{id nameWithOwner}}}}";
    calls.push(graphql(
        mutation,
        json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
        json!({"data":{"closePullRequest":{"pullRequest":node.take()}}}),
    ));
    let mut closed = state_parent();
    closed["state"] = json!("closed");
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), closed.clone()));
    let mut fresh = visibility_with_comments_v19(json!([]));
    for role in [2usize, 8] {
        fresh[role].stdout = serde_json::to_vec(&closed).unwrap();
    }
    calls.extend(fresh);
    assert_eq!(calls.len(), 22);
    calls
}
fn source_tail(personal: bool) -> Vec<GhStep> {
    let mut calls = principal(42);
    if !personal {
        calls.push(get(&format!("repos/{FIRST}"), repository(0)));
        calls.extend(principal(42));
    }
    for qualifier in ["author", "assignee", "review-requested"] {
        let q = if personal {
            format!("q=is:pr is:open {qualifier}:octocat")
        } else {
            format!("q=is:pr is:open {qualifier}:octocat repo:{FIRST}")
        };
        let mut call = get(
            "search/issues",
            json!({"total_count":0,"incomplete_results":false,"items":[]}),
        );
        call.args.extend([
            "-f".into(),
            q,
            "-f".into(),
            "per_page=100".into(),
            "-f".into(),
            "page=1".into(),
        ]);
        calls.push(call);
    }
    if !personal {
        calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    }
    calls.extend(principal(42));
    assert_eq!(calls.len(), if personal { 7 } else { 11 });
    calls
}
fn expected_source(
    mut before: SqlRows,
    source: &str,
    generation: u64,
    fetched: u64,
    catalogue: Option<&str>,
) -> SqlRows {
    let own = |r: &Vec<S>| r[0] == t(HOST) && r[1] == t("user-id:42");
    // Derive the existing trigger effects from the exact PRE image, not an
    // observed poststate or a fixed epoch value. Reservation UPDATE fires
    // OLD+NEW for each bound source; the two binding UPDATEs each fire once.
    let mut lookup_deltas = BTreeMap::<(String, String), i64>::new();
    for binding in before["sync_resource_bindings"].iter().filter(|r| own(r)) {
        let (S::Text(namespace), S::Text(repository)) = (&binding[3], &binding[5]) else {
            panic!("Actual PRE binding namespace/repository must be typed text");
        };
        let reserved = before["sync_reservations"]
            .iter()
            .any(|r| own(r) && r[2] == binding[2]);
        *lookup_deltas
            .entry((namespace.clone(), repository.clone()))
            .or_default() += 2 + 2 * i64::from(reserved);
    }
    for ((namespace, repository), delta) in lookup_deltas {
        let epochs = before.get_mut("sync_binding_lookup_epochs").unwrap();
        if let Some(row) = epochs
            .iter_mut()
            .find(|r| own(r) && r[2] == t(&namespace) && r[3] == t(&repository))
        {
            let S::Integer(generation) = &mut row[4] else {
                panic!("Actual PRE lookup epoch must be an integer");
            };
            *generation += delta;
        } else {
            epochs.push(vec![
                t(HOST),
                t("user-id:42"),
                t(&namespace),
                t(&repository),
                S::Integer(delta),
            ]);
        }
    }
    for r in before.get_mut("sync_reservations").unwrap() {
        if own(r) {
            let S::Integer(g) = &mut r[3] else {
                panic!("actual integer reservation")
            };
            *g += 1;
            r[4] = S::Integer(0);
        }
    }
    let rs = before.get_mut("sync_reservations").unwrap();
    if let Some(r) = rs.iter_mut().find(|r| own(r) && r[2] == t(source)) {
        r[3] = S::Integer(generation as i64);
        r[4] = S::Integer(1);
    } else {
        rs.push(vec![
            t(HOST),
            t("user-id:42"),
            t(source),
            S::Integer(generation as i64),
            S::Integer(1),
        ]);
    }
    for r in before.get_mut("sync_resource_bindings").unwrap() {
        if own(r) {
            r[9] = S::Null;
            r[10] = S::Integer(0);
        }
    }
    let old = before["checkpoint_records"]
        .iter()
        .filter(|r| own(r) && r[2] == t(source))
        .map(|r| (r[3].clone(), r[4].clone()))
        .collect::<Vec<_>>();
    let members = before.get_mut("checkpoint_records").unwrap();
    members.retain(|r| !(own(r) && r[2] == t(source)));
    members.push(vec![
        t(HOST),
        t("user-id:42"),
        t(source),
        t("inbox-meta"),
        t(source),
    ]);
    let retained = members.clone();
    let records = before.get_mut("records").unwrap();
    records.retain(|r| {
        !own(r)
            || !old.contains(&(r[2].clone(), r[3].clone()))
            || retained
                .iter()
                .any(|m| own(m) && m[3] == r[2] && m[4] == r[3])
    });
    records.retain(|r| !(own(r) && r[2] == t("inbox-meta") && r[3] == t(source)));
    records.push(vec![
        t(HOST),
        t("user-id:42"),
        t("inbox-meta"),
        t(source),
        t(""),
        t("{\"coverage\":{\"loaded\":0,\"total_matches\":0,\"complete\":true,\"reasons\":[]}}"),
        S::Integer(fetched as i64),
        S::Null,
        S::Integer(1),
    ]);
    let cps = before.get_mut("checkpoints").unwrap();
    cps.retain(|r| !(own(r) && r[2] == t(source)));
    cps.push(vec![
        t(HOST),
        t("user-id:42"),
        t(source),
        S::Integer(generation as i64),
        S::Integer(fetched as i64),
        S::Integer(1),
    ]);
    let fences = before["pr_write_state_filter_fences_v68"].clone();
    let wms = before
        .get_mut("pr_write_state_source_watermarks_v73")
        .unwrap();
    for f in fences.into_iter().filter(own) {
        wms.retain(|w| !(own(w) && w[2] == f[2] && w[3] == t(source)));
        wms.push(vec![
            t(HOST),
            t("user-id:42"),
            f[2].clone(),
            t(source),
            S::Integer(generation as i64),
            f[6].clone(),
            f[3].clone(),
            f[4].clone(),
        ]);
    }
    // Only the actual selected Catalogue source's LOCAL reservation continues.
    // Its unchanged payload/checkpoint/Read is not a renewed repository access.
    // Exact independent expected candidate identity comes from its sole
    // original catalogue-intent reservation, never an observed poststate.
    for r in before.get_mut("sync_reservations").unwrap() {
        if own(r) && catalogue.is_some_and(|source| r[2] == t(source)) {
            let S::Integer(g) = &mut r[3] else {
                panic!("actual catalogue reservation")
            };
            *g += 1;
            r[4] = S::Integer(1);
        }
    }
    for rows in before.values_mut() {
        rows.sort_by_cached_key(|r| format!("{r:?}"));
    }
    before
}
fn next_generation(f: &Fixture, source: &str) -> u64 {
    let(r,p):(Option<u64>,Option<u64>)=raw(f).query_row("SELECT (SELECT generation FROM sync_reservations WHERE host=?1 AND login='user-id:42' AND source=?2),(SELECT generation FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2)",[HOST,source],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    (r.unwrap_or(0) + u64::from(r.is_some())).max(p.unwrap_or(0)) + 1
}
fn source_name(f: &Fixture) -> String {
    let names=raw(f).prepare("SELECT source FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source LIKE 'personal-inbox:%' ORDER BY source").unwrap().query_map([HOST],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(names.len(), 1);
    names[0].clone()
}
fn actual_terminal(f: &Fixture) {
    let db = raw(f);
    let rows=db.prepare("SELECT a.attempt_id,a.command,a.state,a.pr_node,o.repository_id,o.pr_id,o.issue_id,o.origin_json,d.kind,d.receipt_json,a.confirmed_json FROM pr_write_attempts a JOIN pr_write_state_origins_v68 o USING(attempt_id) JOIN pr_write_state_dispositions_v68 d USING(attempt_id)").unwrap().query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,u64>(4)?,r.get::<_,u64>(5)?,r.get::<_,u64>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?,r.get::<_,String>(9)?,r.get::<_,String>(10)?))).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(
        (&r.1, &r.2, &r.3, r.4, r.5, r.6, r.8.as_str()),
        (
            &"state-close".to_owned(),
            &"confirmed".to_owned(),
            &"GENUINE_CANONICAL_PR_NODE_1007".to_owned(),
            10000,
            1007,
            2007,
            "confirmed_effect"
        )
    );
    let origin: Value = serde_json::from_str(&r.7).unwrap();
    let receipt: Value = serde_json::from_str(&r.9).unwrap();
    let effect: Value = serde_json::from_str(&r.10).unwrap();
    assert_eq!(origin["reference"]["attempt_id"], r.0);
    assert_eq!(origin["seed"]["head"], "a".repeat(40));
    assert_eq!(origin["seed"]["base"], "b".repeat(40));
    assert_eq!(receipt["reference"], origin["reference"]);
    assert_eq!(receipt["old_state"], "open");
    assert_eq!(receipt["new_state"], "closed");
    assert_eq!(receipt["merged"], false);
    assert_eq!(effect["resource_id"], "state-v68/close/closed");
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'state-v68/%'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0,
        "Actual typed closed publication removed only its own exact bodyless draft"
    );
}
fn complete_source(mouse: bool) {
    for personal in [false, true] {
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(state_phase(json!(true)));
        let opened = steps.len();
        steps.extend(state_phase(json!(true)));
        let preview = steps.len();
        steps.extend(confirmed_tail());
        let confirmed = steps.len();
        let f = resource_configured(&steps);
        let sentinel_times = confirm_sql_v86::seed_preserved_rows(&f);
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        enter(
            &mut s,
            &f,
            personal,
            mouse,
            initial,
            false,
            &steps[..initial],
        );
        let barrier = s.completed_cells_end();
        crate::pr_actions::menu(&mut s, mouse, "Close without merging");
        s.wait_for_command_count(&f, opened);
        state_form(&mut s, barrier, false);
        saved_bodyless(&mut s, &f);
        let barrier = s.completed_cells_end();
        control(&mut s, mouse, &state_title(), "[Preview]", STATE_INTENT);
        s.wait_for_command_count(&f, preview);
        state_form(&mut s, barrier, true);
        resize_state(&mut s, 120, 40, STATE_PREVIEW);
        let body =
            crate::support::overlay_body(s.completed_cells_screen().unwrap(), &state_title())
                .unwrap();
        assert!(
            body.contains("repository #10000")
                && body.contains("PR #1007")
                && body.contains("requested closed")
        );
        let confirm_sql = confirm_sql_v86::ConfirmSqlV86::capture(&f, personal, sentinel_times);
        control(
            &mut s,
            mouse,
            &state_title(),
            "[Confirm action]",
            STATE_PREVIEW,
        );
        s.wait_for_command_count(&f, confirmed);
        refreshed_v19::wait_fresh_page_v19(&mut s);
        let mut expected_sql = confirm_sql.assert_confirmed(&f);
        actual_terminal(&f);
        exact(&s, &f, &steps);
        let barrier = s.completed_cells_end();
        choose(&mut s, mouse, "[b Back]", b"b");
        fresh_cells(
            &mut s,
            barrier,
            |screen| {
                crate::support::main_footer_visible(screen)
                    && screen.contents().contains("counts unavailable")
                    && !screen.contents().contains("[b Back]")
                    && !screen.contents().contains(DETAIL_BODY)
            },
            "NEW exact State-unavailable Inbox after actual Back",
        );
        exact(&s, &f, &steps);
        assert_eq!(git_state(&f), git);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        let source = source_name(&f);
        for _ in 0..2 {
            assert_eq!(
                all_rows(&f),
                expected_sql,
                "Actual Back/idle source changes no Confirm SQL"
            );
            confirm_sql.assert_auxiliary_unchanged(&f);
            let stable = expected_sql.clone();
            let catalogue_rows = stable["sync_reservations"]
                .iter()
                .filter(|r| r[0] == t(HOST) && r[1] == t("user-id:42"))
                .filter_map(|r| match &r[2] {
                    S::Text(s) if s.starts_with("repository-catalogue-source-v1:") => {
                        Some(s.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(catalogue_rows.len(), usize::from(!personal));
            let catalogue = catalogue_rows.first().map(String::as_str);
            let generation = next_generation(&f, &source);
            let start = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            steps.extend(source_tail(personal));
            f.scripted_gh(&steps);
            let barrier = s.completed_cells_end();
            choose(&mut s, mouse, "[r Refresh]", b"r");
            s.wait_for_command_count(&f, steps.len());
            fresh_cells(
                &mut s,
                barrier,
                |screen| {
                    let text = screen.contents();
                    crate::support::main_footer_visible(screen)
                        && text.contains("Fresh complete State queue observation")
                        && !text.contains("counts unavailable")
                        && !text.contains("[Esc Cancel]")
                        && !text.contains("[b Back]")
                        && !text.contains(DETAIL_BODY)
                },
                "NEW genuine complete State Inbox after final principal",
            );
            let(g,fetched,complete):(u64,u64,bool)=raw(&f).query_row("SELECT generation,fetched_at,complete FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2",[HOST,source.as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
            assert_eq!(g, generation);
            assert!(complete);
            let end = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            assert!((start..=end).contains(&fetched));
            expected_sql = expected_source(stable, &source, generation, fetched, catalogue);
            assert_eq!(all_rows(&f), expected_sql);
            confirm_sql.assert_auxiliary_unchanged(&f);
            actual_terminal(&f);
            exact(&s, &f, &steps);
            assert_eq!(git_state(&f), git);
            assert_eq!(
                fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
                config
            );
        }
        assert_eq!(
            steps
                .iter()
                .filter(|c| c.stdin.windows(8).any(|v| v == b"mutation"))
                .count(),
            1
        );
        assert_read_only_git(&f);
        s.quit_and_assert_restored_terminal(mouse);
        assert_eq!(
            all_rows(&f),
            expected_sql,
            "Actual Quit changes no derived SQL"
        );
        confirm_sql.assert_auxiliary_unchanged(&f);
    }
}
#[test]
fn pr_lifecycle_v68_keyboard_actual_close_back_and_repeated_complete_source_refresh_preserve_exact_effects(
) {
    complete_source(false)
}
#[test]
fn pr_lifecycle_v68_mouse_actual_close_back_and_repeated_complete_source_refresh_preserve_exact_effects(
) {
    complete_source(true)
}
