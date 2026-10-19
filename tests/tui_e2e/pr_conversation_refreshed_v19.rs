//! Source-only migration of the EIGHT existing sequential Conversation native
//! scenarios. Not aliases to the isolated six-effect tests. Every transition
//! uses the actual current page, original Core, strict paid receipt+detail
//! cursor and same existing journal/queue. Tests/fixtures are NOT authority.
use super::*;
#[path = "pr_conversation_atomic_v85.rs"]
pub(super) mod atomic_v85;
#[path = "pr_conversation_refresh_retry_v20.rs"]
mod retry_v20;
use ghtui::providers::runner::CancellationToken;
use ghtui::search::{
    collaboration::{collect_cached, DiscussionLimits, DiscussionTarget, ParentKind},
    model::DiscussionKind,
};

const FIRST_BODY: &str = "Native café comment 雪\nSecond line";
const EDITED_BODY: &str = "Edited exact comment 雪";
const RETRY_BODY: &str = "Deliberate NEW text after fresh inspection";

fn actual_comment(id: u64, body: &str) -> Value {
    let mut raw = comment(id, body);
    raw["html_url"] = json!(format!("https://{HOST}/{FIRST}/pull/7#issuecomment-{id}"));
    raw
}
fn capabilities_for(raw: &Value, permission: Option<bool>) -> Value {
    let mut result = capabilities(permission);
    let node = &mut result["data"]["node"];
    node["id"] = raw["node_id"].clone();
    node["fullDatabaseId"] = json!(raw["id"].as_u64().unwrap().to_string());
    node["body"] = raw["body"].clone();
    node["createdAt"] = raw["created_at"].clone();
    node["updatedAt"] = raw["updated_at"].clone();
    node["viewerDidAuthor"] = json!(raw["user"]["id"] == 42);
    result
}
fn subscription_for(state: Value, allowed: bool) -> Value {
    let mut value = subscription("UNSUBSCRIBED");
    value["data"]["node"]["viewerSubscription"] = state;
    value["data"]["node"]["viewerCanSubscribe"] = json!(allowed);
    value
}
fn current_phase(
    command: PrCommand,
    opening: bool,
    raw: &Value,
    state: Value,
    allowed: bool,
    actual_reactions: &[Value],
) -> Vec<GhStep> {
    let mut calls = phase(PrCommand::NewComment, false, Some(true));
    let extra = match command {
        PrCommand::NewComment => vec![],
        PrCommand::Subscription => vec![graphql(
            ghtui::providers::gh::PR_CURRENT_SUBSCRIPTION_QUERY,
            json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
            subscription_for(state, allowed),
        )],
        _ => {
            let id = raw["id"].as_u64().unwrap();
            let mut reads = vec![
                get(&format!("repos/{FIRST}/issues/comments/{id}"), raw.clone()),
                graphql(
                    COMMENT_QUERY,
                    json!({"id":raw["node_id"]}),
                    capabilities_for(raw, Some(allowed)),
                ),
            ];
            if command == PrCommand::RemoveReaction || opening && command == PrCommand::AddReaction
            {
                reads.push(get(
                    &format!("repos/{FIRST}/issues/comments/{id}/reactions?per_page=100&page=1"),
                    json!(actual_reactions),
                ));
            }
            reads
        }
    };
    calls.splice(4..4, extra);
    calls
}
fn current_receipt(
    command: PrCommand,
    raw: &Value,
    body: &str,
    actual_reactions: &[Value],
) -> Vec<GhStep> {
    let id = raw["id"].as_u64().unwrap();
    let endpoint = format!("repos/{FIRST}/issues/comments/{id}");
    match command {
        PrCommand::NewComment => vec![
            write(
                &format!("repos/{FIRST}/issues/7/comments"),
                "POST",
                Some(json!({"body":body})),
                raw.clone(),
            ),
            get(&endpoint, raw.clone()),
        ],
        PrCommand::EditComment => vec![
            write(&endpoint, "PATCH", Some(json!({"body":body})), raw.clone()),
            get(&endpoint, raw.clone()),
        ],
        PrCommand::DeleteComment => vec![
            http(&endpoint, "DELETE", None, 204, None),
            graphql(
                ABSENT_QUERY,
                json!({"id":raw["node_id"]}),
                json!({"data":{"node":null}}),
            ),
        ],
        PrCommand::AddReaction => vec![
            http(
                &format!("{endpoint}/reactions"),
                "POST",
                Some(json!({"content":"heart"})),
                201,
                Some(reaction(501, "heart", 42)),
            ),
            get(
                &format!("{endpoint}/reactions?per_page=100&page=1"),
                json!(actual_reactions),
            ),
        ],
        PrCommand::RemoveReaction => vec![
            http(
                &format!("{endpoint}/reactions/501"),
                "DELETE",
                None,
                204,
                None,
            ),
            get(
                &format!("{endpoint}/reactions?per_page=100&page=1"),
                json!(actual_reactions),
            ),
        ],
        PrCommand::Subscription => vec![
            graphql(
                SUB_MUTATION,
                json!({"input":{"subscribableId":"GENUINE_CANONICAL_PR_NODE_1007","state":"UNSUBSCRIBED"}}),
                json!({"data":{"updateSubscription":{"subscribable":{"id":"GENUINE_CANONICAL_PR_NODE_1007","viewerSubscription":"UNSUBSCRIBED"}}}}),
            ),
            graphql(
                ghtui::providers::gh::PR_CURRENT_SUBSCRIPTION_QUERY,
                json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
                subscription_for(json!("UNSUBSCRIBED"), true),
            ),
        ],
        _ => unreachable!(),
    }
}
fn menu_open(s: &mut Session, f: &Fixture, mouse: bool, command: PrCommand, end: usize) {
    let frame = s.completed_cells_end();
    crate::pr_actions::menu(
        s,
        mouse,
        if command == PrCommand::NewComment {
            "New comment • eligibility UNKNOWN/server-enforced"
        } else {
            command.label()
        },
    );
    s.wait_for_command_count(f, end);
    form_frame(s, frame, &title(command), controls(command));
}
fn fill(s: &mut Session, mouse: bool, command: PrCommand, body: &str) {
    match command {
        PrCommand::NewComment | PrCommand::EditComment => {
            if mouse {
                s.click_owned_form_field("PR INTENT • Preview first", "Comment text");
            }
            s.send_key(b"\x01");
            s.paste(body);
            s.wait_for_screen(body.lines().next().unwrap());
        }
        PrCommand::AddReaction => {
            if mouse {
                s.click_visible_action("[heart]");
            } else {
                for _ in 0..4 {
                    s.send_key(b" ");
                }
            }
        }
        PrCommand::RemoveReaction => {
            assert!(
                !s.screen().contains("reaction 502"),
                "Foreign actor's reaction must never become a remove choice"
            );
            if mouse {
                s.click_visible_action("[heart • @octocat • reaction 501]");
            } else {
                s.send_key(b" ");
            }
        }
        PrCommand::Subscription => {
            if mouse {
                s.click_visible_action("[unsubscribed]");
            } else {
                s.send_key(b" ");
            }
        }
        _ => {}
    }
}
fn preview_current(s: &mut Session, f: &Fixture, mouse: bool, command: PrCommand, end: usize) {
    preview_current_with_controls(s, f, mouse, command, end, controls(command));
}
fn preview_current_with_controls(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: PrCommand,
    end: usize,
    footer: &[&str],
) {
    let before = s.completed_cells_end();
    control(s, mouse, &title(command), "[Preview]", footer);
    s.wait_for_command_count(f, end);
    form_frame(
        s,
        before,
        &title(command),
        &["READ-ONLY PREVIEW", "[Confirm action]"],
    );
    // Real prepared readonly Preview must not adopt body poison or invalidate
    // the original confirmation. This is actual input, never handler injection.
    s.paste("V19_PREPARED_PREVIEW_POISON");
    assert!(!s.screen().contains("V19_PREPARED_PREVIEW_POISON"));
}
fn refreshed(s: &mut Session, f: &Fixture, mouse: bool, command: PrCommand, end: usize) {
    let before = s.completed_cursorless_end();
    control(
        s,
        mouse,
        &title(command),
        "[Confirm action]",
        controls(command),
    );
    s.wait_for_command_count(f, end);
    s.wait_for_new_cursorless_condition(
        before,
        |screen| {
            let text = screen.contents();
            crate::support::main_footer_visible(screen)
                && text.contains("Confirmed • fresh PR/search observation")
                && !text.contains("[Confirm action]")
                && !text.contains("READ-ONLY PREVIEW")
                && crate::support::controls::control_painted(screen, "[b Back]")
        },
        "Actual newly installed source-family PR page after paid detail+UID and atomic SQL",
        Duration::from_secs(8),
    );
}
fn select_current(s: &mut Session, mouse: bool, body: &str) {
    let after = s.completed_cursorless_end();
    choose(s, mouse, "[c Conversation]", b"c");
    s.wait_for_new_cursorless_condition(
        after,
        |screen| {
            crate::support::main_footer_visible(screen)
                && crate::support::controls::control_painted(screen, "[c Conversation]")
        },
        "Actual family-preserving cached Conversation page",
        Duration::from_secs(8),
    );
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(5));
        if s.completed_cursorless_screen()
            .is_some_and(|screen| screen.contents().contains(body))
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual refreshed comment not reachable\n{}",
            s.screen()
        );
        let frame = s.completed_cursorless_end();
        if mouse {
            s.wheel(30, 12, true);
        } else {
            s.send_key(b"\x1b[B");
        }
        s.wait_for_new_cursorless_condition(
            frame,
            crate::support::main_footer_visible,
            "One actual Conversation scroll",
            Duration::from_secs(8),
        );
    }
    if mouse {
        s.click_visible_action(body);
    } else {
        for _ in 0..48 {
            if s.screen()
                .lines()
                .any(|line| line.contains(body) && line.contains('>'))
            {
                break;
            }
            let frame = s.completed_cursorless_end();
            s.send_key(b"\x1b[B");
            s.wait_for_new_cursorless_condition(
                frame,
                crate::support::main_footer_visible,
                "One actual comment selection event",
                Duration::from_secs(8),
            );
        }
    }
    s.try_wait_for_condition(
        |screen| {
            screen
                .lines()
                .any(|line| line.contains(body) && line.contains('>'))
        },
        "Actual exact refreshed comment selected",
        Duration::from_secs(8),
    )
    .unwrap();
}

pub(super) fn assert_projection_v19(
    f: &Fixture,
    comments: &[(u64, &str)],
    absent: &[u64],
    attempts: u64,
) {
    let cache = Cache::open(&database(f)).unwrap();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let node = "GENUINE_CANONICAL_PR_NODE_1007";
    let frame = cache
        .sync_frame(&scope, &format!("pr-write-projection:{node}"))
        .unwrap()
        .expect("Actual canonical fresh derived source");
    let last: u64 = raw(f)
        .query_row(
            "SELECT max(attempt_id) FROM pr_write_attempts WHERE state='confirmed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(frame.generation, last);
    assert_eq!(
        raw(f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        attempts
    );
    let details = cache.get(&scope, "pr-details", node).unwrap().unwrap();
    assert_eq!(details.payload["summary"]["pr"]["body"], summary()["body"]);
    assert_eq!(details.payload["summary"]["head_sha"], "a".repeat(40));
    let all = details.payload["comments"].as_array().unwrap();
    assert_eq!(all.len(), comments.len());
    for (id, body) in comments {
        let record = cache
            .get(&scope, "pr-conversation-comment", &format!("IC_{id}"))
            .unwrap()
            .expect("Exact newly observed comment record");
        assert_eq!(record.payload["version"], 1);
        assert_eq!(record.payload["parent_pr_node"], node);
        assert_eq!(record.payload["repository"], FIRST);
        assert_eq!(record.payload["number"], 7);
        assert_eq!(record.payload["comment_id"], *id);
        assert_eq!(record.payload["body"], *body);
        assert!(all.iter().any(|c| c["id"] == *id && c["body"] == *body));
        assert!(frame.records.iter().any(|r| r.id == format!("IC_{id}")));
    }
    for id in absent {
        assert!(cache
            .get(&scope, "pr-conversation-comment", &format!("IC_{id}"))
            .unwrap()
            .is_none());
        assert!(!all.iter().any(|c| c["id"] == *id));
    }
    let target = DiscussionTarget {
        scope: scope.clone(),
        repository_id: 10000,
        repository: FIRST.into(),
        parent_kind: ParentKind::PullRequest,
        parent_node: node.into(),
        number: 7,
        category: DiscussionKind::ConversationComment,
    };
    let indexed = collect_cached(
        &cache,
        scope,
        &[target],
        &DiscussionLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(indexed.corpus.documents.len(), comments.len());
    for (_, body) in comments {
        assert!(indexed.corpus.documents.iter().any(|d|d.body==*body),"Fresh body must be admitted into the real canonical search corpus, not only the UI/receipt");
    }
    for id in absent {
        assert!(!indexed
            .corpus
            .documents
            .iter()
            .any(|d| d.key.resource == format!("IC_{id}")));
    }
}
fn protected(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    ["SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'conversation-draft-v17/%' ORDER BY host,login,resource","SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name","SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id","SELECT json_array(host,login,kind,id,payload) FROM records WHERE login!='user-id:42' ORDER BY host,login,kind,id"].iter().flat_map(|sql|db.prepare(sql).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
pub(super) fn wait_fresh_page_v19(s: &mut Session) {
    s.wait_for_cursorless_condition(
        |screen| {
            let text = screen.contents();
            crate::support::main_footer_visible(screen)
                && text.contains("Confirmed • fresh PR/search observation")
                && crate::support::controls::control_painted(screen, "[b Back]")
                && !text.contains("[Confirm action]")
                && !text.contains("READ-ONLY PREVIEW")
        },
        "Actual fresh original-family PR page after terminal UID and exact atomic projection",
        Duration::from_secs(8),
    );
}
/// Preserve ALL stable SQL outside the three explicitly authorized derived
/// sources. Every admitted row is additionally checked against exact target,
/// known node set, actual body/receipt and real search visibility; no blanket
/// host/table exclusion or adopting observed poststate as an expected baseline.
pub(super) fn assert_raw_transition_v19(f: &Fixture, before: &[String]) {
    let scope = Scope::github(HOST, Some(42)).unwrap();
    let node = "GENUINE_CANONICAL_PR_NODE_1007";
    let sources = [
        DiscussionKind::PullRequestBody,
        DiscussionKind::ConversationComment,
    ]
    .map(|category| {
        DiscussionTarget {
            scope: scope.clone(),
            repository_id: 10000,
            repository: FIRST.into(),
            parent_kind: ParentKind::PullRequest,
            parent_node: node.into(),
            number: 7,
            category,
        }
        .source()
    });
    let projection = format!("pr-write-projection:{node}");
    let authorized = |row: &String| {
        let v: Value = serde_json::from_str(row).unwrap();
        let Some(a) = v.as_array() else {
            return false;
        };
        if a.len() != 5 || a[0] != HOST || a[1] != "user-id:42" {
            return false;
        }
        if a[2]
            .as_str()
            .is_some_and(|s| s == projection || sources.iter().any(|p| p == s))
            && a[3].is_number()
        {
            return true;
        }
        let Some(kind) = a[2].as_str() else {
            return false;
        };
        let Some(id) = a[3].as_str() else {
            return false;
        };
        match kind {
            "pr-details" => id == node,
            "pr-conversation-comment" => {
                ["IC_501", "IC_901", "IC_902", "IC_10", "IC_11"].contains(&id)
            }
            "collaboration-state-v1" => sources.iter().any(|s| s == id),
            "collaboration-document-v1" => {
                let payload: Value = serde_json::from_str(a[4].as_str().unwrap()).unwrap();
                let document: ghtui::search::model::Document =
                    serde_json::from_value(payload).unwrap();
                assert_eq!(document.key.stable_id(), id);
                assert_eq!(document.key.scope, scope);
                document.metadata.discussion.as_ref().is_some_and(|c| {
                    c.parent_node == node
                        && c.repository_id == 10000
                        && c.parent_number == 7
                        && matches!(
                            c.kind,
                            DiscussionKind::PullRequestBody | DiscussionKind::ConversationComment
                        )
                })
            }
            _ => false,
        }
    };
    let after = stable_sql(f);
    assert_eq!(after.iter().filter(|r|!authorized(r)).collect::<Vec<_>>(),before.iter().filter(|r|!authorized(r)).collect::<Vec<_>>(),"Every other stable raw SQL row must be byte-identical to the independently captured pre-operation state");
    for source in &sources {
        let (generation,complete):(u64,i64)=raw(f).query_row("SELECT generation,complete FROM checkpoints WHERE host=?1 AND login='user-id:42' AND source=?2",[HOST,source.as_str()],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert!(generation > 0);
        assert_eq!(
            complete, 0,
            "Unknown parent-label coverage remains honest, not fabricated complete"
        );
        let (reservation,visible,active):(u64,u64,i64)=raw(f).query_row("SELECT reservation_generation,visible_generation,active FROM sync_resource_bindings WHERE host=?1 AND login='user-id:42' AND source=?2 AND repository_id=10000 AND resource='GENUINE_CANONICAL_PR_NODE_1007'",[HOST,source.as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!((reservation, visible, active), (generation, generation, 1));
    }
}
pub(super) fn assert_published_transition_v19(
    f: &Fixture,
    before: &[String],
    command: PrCommand,
    attempts: u64,
) {
    assert_raw_transition_v19(f, before);
    let expected = match command {
        PrCommand::NewComment => vec![(501, OLD), (901, BODY)],
        PrCommand::EditComment => vec![(501, BODY)],
        PrCommand::DeleteComment => vec![],
        _ => vec![(501, OLD)],
    };
    assert_projection_v19(
        f,
        &expected,
        if command == PrCommand::DeleteComment {
            &[501]
        } else {
            &[]
        },
        attempts,
    );
}
fn seed_independent(f: &Fixture) {
    for uid in [42, 43] {
        Cache::open(&database(f))
            .unwrap()
            .put_draft(
                &Scope::github(HOST, Some(uid)).unwrap(),
                "V19-independent-kept",
                &format!("PRIVATE_{uid}"),
            )
            .unwrap();
    }
}
fn confirm_receipts(f: &Fixture, expected: &[(&str, &str)]) {
    let db = raw(f);
    let rows = db
        .prepare("SELECT command,state,confirmed_json FROM pr_write_attempts ORDER BY attempt_id")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), expected.len());
    for ((command, state, effect), (purpose, id)) in rows.iter().zip(expected) {
        assert_eq!(command, purpose);
        assert_eq!(state, "confirmed");
        assert_eq!(
            serde_json::from_str::<Value>(effect).unwrap()["resource_id"],
            *id
        );
    }
}

/// Original paired native scenario: Preview Close persists exact editable
/// intent, never approval. Reopen/Preview/Confirm publishes UI AND search.
pub(crate) fn create_cancel_receipt(mouse: bool) {
    let command = PrCommand::NewComment;
    let created = actual_comment(10, FIRST_BODY);
    let mut steps = initial_steps(true);
    let initial = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let open1 = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let preview1 = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let open2 = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let preview2 = steps.len();
    steps.extend(phase(command, false, Some(true)));
    steps.extend(current_receipt(command, &created, FIRST_BODY, &[]));
    steps.extend(visibility_with_comments_v19(json!([
        comment(501, OLD),
        created
    ])));
    let f = resource_configured(&steps);
    seed_independent(&f);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
    let kept = protected(&f);
    menu_open(&mut s, &f, mouse, command, open1);
    fill(&mut s, mouse, command, FIRST_BODY);
    preview_current(&mut s, &f, mouse, command, preview1);
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    let frame = s.completed_cursorless_end();
    control(
        &mut s,
        mouse,
        &title(command),
        "[Keep draft / Close]",
        EDITABLE,
    );
    s.wait_for_new_cursorless_condition(
        frame,
        |screen| {
            crate::support::main_footer_visible(screen)
                && !screen.contents().contains("[Confirm action]")
        },
        "Original page after pure Preview Close",
        Duration::from_secs(8),
    );
    let (key, encoded): (String, String) = raw(&f)
        .query_row(
            "SELECT resource,body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let draft: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(draft["values"]["body"], FIRST_BODY);
    assert!(draft["publication"].is_null());
    assert!(draft.get("approval").is_none());
    exact(&s, &f, &steps[..preview1]);
    assert_eq!(protected(&f), kept);
    menu_open(&mut s, &f, mouse, command, open2);
    s.wait_for_screen("Native café comment 雪");
    assert!(
        !s.screen().contains("READ-ONLY PREVIEW"),
        "Restored body never restores approval"
    );
    let before_resize = s.completed_cells_end();
    s.resize(80, 24);
    form_frame(&mut s, before_resize, &title(command), EDITABLE);
    preview_current(&mut s, &f, mouse, command, preview2);
    refreshed(&mut s, &f, mouse, command, steps.len());
    assert_projection_v19(&f, &[(501, OLD), (10, FIRST_BODY)], &[], 1);
    select_current(&mut s, mouse, "Native café comment 雪");
    if !s.screen().contains("Second line") {
        // At 80x24 the merged header leaves the selected first body line as
        // the last visible row. ONE normal Down selects its next row, which
        // carries the SAME actual comment selection; ONE wheel notch scrolls
        // the viewport and keeps the first line selected. A new owned frame
        // must expose BOTH exact lines.
        let before = s.completed_cursorless_end();
        if mouse {
            s.wheel(30, 12, true);
        } else {
            s.send_key(b"\x1b[B");
        }
        s.wait_for_new_cursorless_condition(
            before,
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains("Inbox • focused view")
                    && crate::support::controls::control_painted(screen, "[c Conversation]")
                    && text.contains("Native café comment 雪")
                    && text.contains(if mouse {
                        "> Native café comment 雪"
                    } else {
                        "> Second line"
                    })
                    && text.contains("Second line")
                    && text.contains("Selected comment #10 by @octocat")
                    && !text.contains("[Confirm action]")
            },
            "NEW owned Conversation frame shows both exact lines of selected actual comment10",
            Duration::from_secs(8),
        );
    }
    s.wait_for_screen("Second line");
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT count(*) FROM drafts WHERE resource=?1",
                [&key],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    confirm_receipts(&f, &[(command_key(command), "10")]);
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

#[derive(Clone, Copy)]
struct RegionV19 {
    opened: usize,
    preview: usize,
    end: usize,
}
const SEQUENTIAL_V19: [PrCommand; 6] = [
    PrCommand::NewComment,
    PrCommand::EditComment,
    PrCommand::AddReaction,
    PrCommand::RemoveReaction,
    PrCommand::DeleteComment,
    PrCommand::Subscription,
];
/// Original paired LONG scenario: one current page and actual successors,
/// six writes, owned removal, deletion pruning and fresh subscription reopen.
pub(crate) fn sequential_six(mouse: bool) {
    let created = actual_comment(10, "Comment before native edit");
    let edited = actual_comment(10, EDITED_BODY);
    let owned = reaction(501, "heart", 42);
    let foreign = reaction(502, "heart", 43);
    let mut steps = initial_steps(true);
    let initial = steps.len();
    let mut regions = Vec::new();
    for command in SEQUENTIAL_V19 {
        let old = if command == PrCommand::EditComment {
            &created
        } else {
            &edited
        };
        let reactions = if command == PrCommand::RemoveReaction {
            vec![owned.clone(), foreign.clone()]
        } else {
            vec![]
        };
        for opening in [true, false, false] {
            steps.extend(current_phase(
                command,
                opening,
                old,
                json!("SUBSCRIBED"),
                true,
                &reactions,
            ));
            if opening {
                regions.push(RegionV19 {
                    opened: steps.len(),
                    preview: 0,
                    end: 0,
                });
            } else if regions.last().unwrap().preview == 0 {
                regions.last_mut().unwrap().preview = steps.len();
            }
        }
        let receipt_comment = if command == PrCommand::NewComment {
            &created
        } else {
            &edited
        };
        let after_reactions = if command == PrCommand::AddReaction {
            vec![owned.clone(), foreign.clone()]
        } else {
            vec![foreign.clone()]
        };
        steps.extend(current_receipt(
            command,
            receipt_comment,
            if command == PrCommand::NewComment {
                "Comment before native edit"
            } else {
                EDITED_BODY
            },
            &after_reactions,
        ));
        steps.extend(visibility_with_comments_v19(match command {
            PrCommand::NewComment => json!([comment(501, OLD), created.clone()]),
            PrCommand::DeleteComment | PrCommand::Subscription => json!([comment(501, OLD)]),
            _ => json!([comment(501, OLD), edited.clone()]),
        }));
        regions.last_mut().unwrap().end = steps.len();
    }
    steps.extend(current_phase(
        PrCommand::Subscription,
        true,
        &edited,
        json!("UNSUBSCRIBED"),
        true,
        &[],
    ));
    let reopened = steps.len();
    let f = resource_configured(&steps);
    seed_independent(&f);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
    let kept = protected(&f);
    for (index, (command, region)) in SEQUENTIAL_V19.into_iter().zip(regions).enumerate() {
        if !matches!(command, PrCommand::NewComment | PrCommand::Subscription) {
            select_current(
                &mut s,
                mouse,
                if command == PrCommand::EditComment {
                    "Comment before native edit"
                } else {
                    EDITED_BODY
                },
            );
        }
        menu_open(&mut s, &f, mouse, command, region.opened);
        if command == PrCommand::EditComment {
            s.wait_for_screen("Comment before native edit");
        }
        fill(
            &mut s,
            mouse,
            command,
            if command == PrCommand::NewComment {
                "Comment before native edit"
            } else {
                EDITED_BODY
            },
        );
        preview_current(&mut s, &f, mouse, command, region.preview);
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            index
        );
        refreshed(&mut s, &f, mouse, command, region.end);
        let expected = match command {
            PrCommand::NewComment => vec![(501, OLD), (10, "Comment before native edit")],
            PrCommand::DeleteComment | PrCommand::Subscription => vec![(501, OLD)],
            _ => vec![(501, OLD), (10, EDITED_BODY)],
        };
        assert_projection_v19(
            &f,
            &expected,
            if matches!(command, PrCommand::DeleteComment | PrCommand::Subscription) {
                &[10]
            } else {
                &[]
            },
            (index + 1) as u64,
        );
        exact(&s, &f, &steps[..region.end]);
        assert_eq!(protected(&f), kept);
        assert_eq!(git_state(&f), git);
    }
    menu_open(&mut s, &f, mouse, PrCommand::Subscription, reopened);
    s.wait_for_screen("Current subscription: unsubscribed");
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        6
    );
    control(
        &mut s,
        mouse,
        &title(PrCommand::Subscription),
        "[Keep draft / Close]",
        BODYLESS,
    );
    confirm_receipts(
        &f,
        &[
            (command_key(PrCommand::NewComment), "10"),
            (command_key(PrCommand::EditComment), "10"),
            (command_key(PrCommand::AddReaction), "501"),
            (command_key(PrCommand::RemoveReaction), "501"),
            (command_key(PrCommand::DeleteComment), "10"),
            (command_key(PrCommand::Subscription), "unsubscribed"),
        ],
    );
    let methods = commands(&s, &f)
        .into_iter()
        .filter_map(|row| {
            let args = row["args"].as_array()?;
            let i = args.iter().position(|v| v == "--method")?;
            let m = args.get(i + 1)?.as_str()?;
            if args[1] == "graphql" {
                let bytes = decode_hex(row["stdin_hex"].as_str()?);
                let payload: Value = serde_json::from_slice(&bytes).unwrap();
                return payload["query"]
                    .as_str()
                    .is_some_and(|q| q.starts_with("mutation"))
                    .then(|| ("graphql".to_owned(), m.to_owned()));
            }
            matches!(m, "POST" | "PATCH" | "DELETE")
                .then(|| (args[1].as_str().unwrap().to_owned(), m.to_owned()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        vec![
            (format!("repos/{FIRST}/issues/7/comments"), "POST".into()),
            (format!("repos/{FIRST}/issues/comments/10"), "PATCH".into()),
            (
                format!("repos/{FIRST}/issues/comments/10/reactions"),
                "POST".into()
            ),
            (
                format!("repos/{FIRST}/issues/comments/10/reactions/501"),
                "DELETE".into()
            ),
            (format!("repos/{FIRST}/issues/comments/10"), "DELETE".into()),
            ("graphql".into(), "POST".into())
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
fn decode_hex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect()
}

/// Original paired nullable/denied subscription scenario. An actual null is
/// blank, not a fabricated default or a toggle. Explicit choices and Preview
/// remain read-only; a fresh actual false capability refuses without a POST.
pub(crate) fn nullable_subscription_denied(mouse: bool) {
    let command = PrCommand::Subscription;
    let comment = actual_comment(10, FIRST_BODY);
    let mut steps = initial_steps(true);
    let initial = steps.len();
    steps.extend(current_phase(
        command,
        true,
        &comment,
        Value::Null,
        true,
        &[],
    ));
    let opened = steps.len();
    steps.extend(current_phase(
        command,
        false,
        &comment,
        Value::Null,
        true,
        &[],
    ));
    let previewed = steps.len();
    let mut denied = current_phase(command, true, &comment, json!("UNSUBSCRIBED"), false, &[]);
    // Denial occurs from the genuine observed typed current capability, before
    // closing reads/source publication, not by accepting a fake closed source.
    denied.truncate(5);
    steps.extend(denied);
    let refused = steps.len();
    let f = resource_configured(&steps);
    seed_independent(&f);
    let git = git_state(&f);
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
    let stable = stable_sql(&f);
    let kept = protected(&f);
    menu_open(&mut s, &f, mouse, command, opened);
    s.wait_for_screen("Current subscription: unavailable");
    let (column, row) = s.locate("Requested subscription state").unwrap();
    s.assert_cells(row + 1, column, "            ");
    let before = s.completed_cells_end();
    control(&mut s, mouse, &title(command), "[Preview]", BODYLESS);
    form_frame(
        &mut s,
        before,
        &title(command),
        &["Choose a supported subscription state", "[Preview]"],
    );
    exact(&s, &f, &steps[..opened]);
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    if mouse {
        s.click_visible_action("[subscribed]");
        s.click_visible_action("[unsubscribed]");
    } else {
        // The failed Preview focused its control. BackTab reaches the actual
        // choice field; Space there chooses values, not another Preview.
        s.send_key(b"\x1b[Z");
        for value in ["subscribed", "unsubscribed"] {
            let before_choice = s.completed_cells_end();
            s.send_key(b" ");
            form_frame(
                &mut s,
                before_choice,
                &title(command),
                &[
                    "Current subscription: unavailable",
                    &format!("Requested subscription state • {value}"),
                    "[Preview]",
                    "[Confirm action]",
                    "[Keep draft / Close]",
                ],
            );
            s.assert_cells(row + 1, column, value);
            // Field paint, not an old Preview-focused frame, acknowledges
            // each choice before the unchanged focus/Preview traversal.
            exact(&s, &f, &steps[..opened]);
            assert_eq!(stable_sql(&f), stable);
            assert_eq!(protected(&f), kept);
        }
    }
    let mut error_footer = BODYLESS.to_vec();
    error_footer.extend(["[F7 Error↑]", "[F8 Error↓]"]);
    preview_current_with_controls(&mut s, &f, mouse, command, previewed, &error_footer);
    s.wait_for_screen("Requested subscription state: unsubscribed");
    control(
        &mut s,
        mouse,
        &title(command),
        "[Keep draft / Close]",
        BODYLESS,
    );
    s.wait_for_cursorless_condition(
        |screen| {
            crate::support::main_footer_visible(screen)
                && !screen.contents().contains("[Confirm action]")
        },
        "Current page after read-only subscription Preview Close",
        Duration::from_secs(8),
    );
    let before = s.completed_cursorless_end();
    crate::pr_actions::menu(&mut s, mouse, command.label());
    s.wait_for_command_count(&f, refused);
    s.wait_for_new_cursorless_condition(
        before,
        |screen| {
            crate::support::main_footer_visible(screen)
                && screen.contents().contains("Actual capability refused")
                && !screen.contents().contains("[Confirm action]")
        },
        "Fresh typed viewerCanSubscribe=false refuses without creating a publication Form",
        Duration::from_secs(8),
    );
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(stable_sql(&f), stable);
    assert_eq!(protected(&f), kept);
    exact(&s, &f, &steps);
    assert_eq!(git_state(&f), git);
    assert_read_only_git(&f);
    s.quit_and_assert_restored_terminal(mouse);
}

fn terminal_main(s: &mut Session, needle: &str) {
    s.wait_for_cursorless_condition(
        |screen| {
            let text = screen.contents();
            crate::support::main_footer_visible(screen)
                && text.contains(needle)
                && !text.contains("[Confirm action]")
                && !text.contains("[Inspect current]")
                && !text.contains(FIRST_BODY)
                && !text.contains(RETRY_BODY)
        },
        "Owned terminal main outcome, not matching busy/Preview body text",
        Duration::from_secs(8),
    );
}
/// Original paired uncertain restart: no automatic resend, fresh inspection
/// alone does not unlock, explicit local ACK preserves old body/ref/origin,
/// then a NEW genuine current phase + explicit V18 New composes fresh text.
pub(crate) fn uncertain_restart_inspect_ack_new(mouse: bool) {
    let command = PrCommand::NewComment;
    let created = actual_comment(11, RETRY_BODY);
    let mut steps = initial_steps(true);
    let initial = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let opened = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let previewed = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let mut denied =
        current_receipt(command, &actual_comment(10, FIRST_BODY), FIRST_BODY, &[]).remove(0);
    denied.stdout.clear();
    denied.status = 1;
    denied.stderr = b"Actual first POST outcome cannot be confirmed".to_vec();
    steps.push(denied);
    let failed = steps.len();
    steps.extend(initial_steps(true));
    let second = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let history = steps.len();
    steps.extend(lineage_v18::inspect_steps(command));
    let inspected = steps.len();
    steps.extend(initial_steps(true));
    let third = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let notice = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let fresh = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let preview2 = steps.len();
    steps.extend(phase(command, false, Some(true)));
    steps.extend(current_receipt(command, &created, RETRY_BODY, &[]));
    steps.extend(visibility_with_comments_v19(json!([
        comment(501, OLD),
        created
    ])));
    let f = resource_configured(&steps);
    seed_independent(&f);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
    let kept = protected(&f);
    menu_open(&mut s, &f, mouse, command, opened);
    fill(&mut s, mouse, command, FIRST_BODY);
    preview_current(&mut s, &f, mouse, command, previewed);
    control(&mut s, mouse, &title(command), "[Confirm action]", EDITABLE);
    s.wait_for_command_count(&f, failed);
    terminal_main(&mut s, "Unknown outcome; inspect");
    let (attempt,key,body,origin,digest):(i64,String,String,String,String)=raw(&f).query_row("SELECT a.attempt_id,d.resource,d.body,o.origin_json,a.payload_sha256 FROM pr_write_attempts a JOIN pr_write_conversation_origins_v17 o USING(attempt_id) JOIN drafts d ON d.resource LIKE 'conversation-draft-v17/%' WHERE a.state='uncertain'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    let encoded: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(encoded["values"]["body"], FIRST_BODY);
    assert_eq!(encoded["publication"]["attempt_id"], attempt);
    assert_eq!(encoded["publication"]["payload_sha256"], digest);
    assert!(encoded.get("approval").is_none());
    assert!(Cache::open(&database(&f))
        .unwrap()
        .sync_frame(
            &Scope::github(HOST, Some(42)).unwrap(),
            "pr-write-projection:GENUINE_CANONICAL_PR_NODE_1007"
        )
        .unwrap()
        .is_none());
    exact(&s, &f, &steps[..failed]);
    s.quit_and_assert_restored_terminal(mouse);
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, second, false, &steps[..second]);
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [attempt],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "uncertain"
    );
    let history_title = format!("Original Conversation history • {FIRST}#7");
    let frame = s.completed_cells_end();
    crate::pr_actions::menu(&mut s, mouse, "Inspect pending result");
    s.wait_for_command_count(&f, history);
    form_frame(&mut s, frame, &history_title, HISTORY);
    let frame = s.completed_cells_end();
    control(&mut s, mouse, &history_title, "[Inspect current]", HISTORY);
    s.wait_for_command_count(&f, inspected);
    form_frame(&mut s, frame, &history_title, HISTORY);
    lineage_v18::scroll(&mut s,mouse,&history_title,"Original created comment identity was never observed; UNKNOWN attribution. No first/newest comment is chosen.");
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [attempt],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "uncertain",
        "Inspection is not ACK or resend"
    );
    control(
        &mut s,
        mouse,
        &history_title,
        "[Acknowledge local]",
        HISTORY,
    );
    terminal_main(&mut s, "Local ACK #");
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [attempt],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "acknowledged"
    );
    exact(&s, &f, &steps[..inspected]);
    assert_eq!(
        raw(&f)
            .query_row("SELECT body FROM drafts WHERE resource=?1", [&key], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
        body
    );
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT origin_json FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
                [attempt],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        origin
    );
    s.quit_and_assert_restored_terminal(mouse);
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, third, false, &steps[..third]);
    let frame = s.completed_cells_end();
    crate::pr_actions::menu(
        &mut s,
        mouse,
        "New comment • eligibility UNKNOWN/server-enforced",
    );
    s.wait_for_command_count(&f, notice);
    form_frame(
        &mut s,
        frame,
        &title(command),
        &["[n New action]", "[Inspect history]", "[Esc Close]"],
    );
    let frame = s.completed_cells_end();
    control(
        &mut s,
        mouse,
        &title(command),
        "[n New action]",
        &["[n New action]", "[Inspect history]", "[Esc Close]"],
    );
    s.wait_for_command_count(&f, fresh);
    form_frame(&mut s, frame, &title(command), EDITABLE);
    assert!(!s.screen().contains(FIRST_BODY));
    fill(&mut s, mouse, command, RETRY_BODY);
    preview_current(&mut s, &f, mouse, command, preview2);
    refreshed(&mut s, &f, mouse, command, steps.len());
    assert_projection_v19(&f, &[(501, OLD), (11, RETRY_BODY)], &[10], 2);
    let states = raw(&f)
        .prepare("SELECT attempt_id,state FROM pr_write_attempts ORDER BY attempt_id")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(states[0], (attempt, "acknowledged".into()));
    assert_eq!(states.len(), 2);
    assert_eq!(states[1].1, "confirmed");
    assert!(states[1].0 > attempt);
    assert_eq!(
        raw(&f)
            .query_row("SELECT body FROM drafts WHERE resource=?1", [&key], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
        body
    );
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT origin_json FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
                [attempt],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        origin
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

fn read_phase_v19() -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    let lists = visibility_with_comments_v19(json!([comment(501, OLD), comment(901, BODY)]));
    calls.extend(lists[3..8].iter().cloned());
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 13);
    calls
}
fn retained_read(mouse: bool, personal: bool) {
    for mode in 0..3 {
        let command = PrCommand::NewComment;
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(phase(command, true, Some(true)));
        let opened = steps.len();
        steps.extend(phase(command, false, Some(true)));
        let previewed = steps.len();
        steps.extend(phase(command, false, Some(true)));
        steps.extend(receipt(command));
        let mut unavailable = visibility();
        unavailable[3].stdout = b"[]".to_vec();
        unavailable.truncate(4);
        steps.extend(unavailable);
        let retained = steps.len();
        if mode != 1 {
            let mut read = read_phase_v19();
            if mode == 2 {
                *read.last_mut().unwrap() = get("user", json!({"id":43,"login":"octocat"}));
            }
            steps.extend(read);
        }
        let read_end = steps.len();
        if mode == 0 {
            steps.extend(phase(PrCommand::Subscription, true, Some(true)));
        }
        let follow = steps.len();
        let f = resource_configured(&steps);
        seed_independent(&f);
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
        let kept = protected(&f);
        prepare(&mut s, &f, mouse, command, opened, previewed);
        control(&mut s, mouse, &title(command), "[Confirm action]", EDITABLE);
        s.wait_for_command_count(&f, retained);
        let title = "Confirmed Conversation • read-only refresh";
        let footer = &["[r Refresh PR]", "[Esc Back to Inbox]"];
        form_frame(
            &mut s,
            0,
            title,
            &["DURABLY CONFIRMED", "[r Refresh PR]", "[Esc Back to Inbox]"],
        );
        let (body,origin,receipt):(String,String,String)=raw(&f).query_row("SELECT d.body,o.origin_json,a.confirmed_json FROM pr_write_attempts a JOIN pr_write_conversation_origins_v17 o USING(attempt_id) JOIN drafts d ON d.resource LIKE 'conversation-draft-v17/%' WHERE a.state='confirmed'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&receipt).unwrap()["resource_id"],
            "901"
        );
        assert!(
            !crate::support::overlay_body(s.completed_cells_screen().unwrap(), title)
                .unwrap()
                .contains(BODY)
        );
        if mode == 1 {
            let db = rusqlite::Connection::open_with_flags(
                database(&f),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap();
            let source: String = if personal {
                db.query_row("SELECT source FROM sync_reservations WHERE source LIKE 'personal-pr-source-v13/%'",[],|r|r.get(0)).unwrap()
            } else {
                db.query_row("SELECT source FROM sync_reservations WHERE source LIKE 'repository-catalogue-source-v1:%'",[],|r|r.get(0)).unwrap()
            };
            assert_eq!(
                db.execute(
                    "UPDATE sync_reservations SET generation=generation+1 WHERE source=?1",
                    [&source]
                )
                .unwrap(),
                1
            );
        }
        let before_read = stable_sql(&f);
        let frame = s.completed_cells_end();
        control(&mut s, mouse, title, "[r Refresh PR]", footer);
        s.wait_for_command_count(&f, read_end);
        match mode {
            0 => {
                s.wait_for_new_cursorless_condition(
                    frame,
                    |screen| {
                        crate::support::main_footer_visible(screen)
                            && screen
                                .contents()
                                .contains("Fresh original PR/search observation")
                            && crate::support::controls::control_painted(screen, "[b Back]")
                            && !screen.contents().contains("[r Refresh PR]")
                    },
                    "NEW13-child actual current page after explicit read-only refresh",
                    Duration::from_secs(8),
                );
                assert_projection_v19(&f, &[(501, OLD), (901, BODY)], &[], 1);
                assert_raw_transition_v19(&f, &before_read);
                menu_open(&mut s, &f, mouse, PrCommand::Subscription, follow);
                control(
                    &mut s,
                    mouse,
                    &super::title(PrCommand::Subscription),
                    "[Keep draft / Close]",
                    BODYLESS,
                );
            }
            1 => {
                form_frame(
                    &mut s,
                    frame,
                    title,
                    &["Back and reopen", "[Esc Back to Inbox]"],
                );
                let exact_before = commands(&s, &f);
                let sql = stable_sql(&f);
                let next = s.completed_cells_end();
                // A second explicit gesture must be visibly disabled/refused,
                // with ZERO new child/derived/journal/draft effects. It cannot
                // rebase an own-reservation or renew the consumed read phase.
                if mouse {
                    let p = s.wait_for_owned_completed_action(title, "[r Refresh PR]", &[]);
                    s.click(p.0, p.1);
                    // Disabled caption has no action hit. A genuine Tab-only
                    // focus repaint acknowledges subsequent input without
                    // guessing that the disabled click must repaint itself.
                    s.send_key(b"\t");
                } else {
                    s.send_key(b"r");
                }
                form_frame(
                    &mut s,
                    next,
                    title,
                    &["Back and reopen", "[Esc Back to Inbox]"],
                );
                assert_eq!(commands(&s, &f), exact_before);
                assert_eq!(stable_sql(&f), sql);
                let before = s.completed_cursorless_end();
                control(&mut s, mouse, title, "[Esc Back to Inbox]", footer);
                s.wait_for_new_cursorless_condition(
                    before,
                    |screen| {
                        crate::support::main_footer_visible(screen)
                            && screen.contents().contains("Cached Inbox only")
                            && !screen.contents().contains(DETAIL_BODY)
                    },
                    "Failure-only cached Back removes old PR page/tickets without reads",
                    Duration::from_secs(8),
                );
                assert_eq!(stable_sql(&f), sql);
            }
            _ => {
                s.wait_for_cursorless_condition(|screen|crate::support::main_footer_visible(screen)&&!screen.contents().contains("[r Refresh PR]")&&!screen.contents().contains(DETAIL_BODY)&&!screen.contents().contains(BODY)&&screen.contents().contains("Original Conversation unavailable"),"Actual final numeric UID loss discards retained private page, never Back-restores it",Duration::from_secs(8));
                assert_eq!(stable_sql(&f), before_read);
            }
        }
        let (still_body,still_origin,still_receipt):(String,String,String)=raw(&f).query_row("SELECT d.body,o.origin_json,a.confirmed_json FROM pr_write_attempts a JOIN pr_write_conversation_origins_v17 o USING(attempt_id) JOIN drafts d ON d.resource LIKE 'conversation-draft-v17/%' WHERE a.state='confirmed'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(
            (still_body, still_origin, still_receipt),
            (body, origin, receipt)
        );
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            raw(&f)
                .query_row(
                    "SELECT count(*) FROM pr_write_attempts WHERE state='acknowledged'",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            0
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
}
#[test]
fn conversation_refresh_v19_catalogue_keyboard_new_read_failure_back_and_uid_privacy() {
    retained_read(false, false)
}
#[test]
fn conversation_refresh_v19_catalogue_mouse_new_read_failure_back_and_uid_privacy() {
    retained_read(true, false)
}
#[test]
fn conversation_refresh_v19_personal_keyboard_new_read_failure_back_and_uid_privacy() {
    retained_read(false, true)
}
#[test]
fn conversation_refresh_v19_personal_mouse_new_read_failure_back_and_uid_privacy() {
    retained_read(true, true)
}
