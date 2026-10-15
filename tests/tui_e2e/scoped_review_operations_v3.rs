//! SOURCE ONLY until Root runs the actual binary. These journeys never invoke
//! an App handler: all selection, Preview, Confirm, resize and Cancel input is
//! sent through the owned PTY, with strict transport and independent SQL/Git.
use super::*;
#[path = "scoped_review_action_lineage_v12.rs"]
mod action_lineage_v12;
const BODY: &str = "Exact native pending replacement 界";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Edit,
    Delete,
    LastDelete,
    User,
    Bot,
    Team,
    Add,
}
impl Operation {
    fn reviewer(self) -> bool {
        matches!(self, Self::User | Self::Bot | Self::Team)
    }
    fn body(self) -> bool {
        matches!(self, Self::Edit | Self::Add)
    }
    fn command(self) -> &'static str {
        match self {
            Self::Edit => "review-pending-comment-edit-v3",
            Self::Delete | Self::LastDelete => "review-pending-comment-delete-v3",
            Self::Add => "review-pending-comment-add-v3",
            _ => "review-rerequest-v3",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Edit => "Edit exact server PENDING comment — not submitted",
            Self::Delete | Self::LastDelete => "Delete exact server PENDING comment",
            Self::Add => "Add comment to exact server PENDING review — not submitted",
            _ => "Re-request actual reviewer — eligibility UNKNOWN",
        }
    }
    fn author(self) -> u64 {
        if matches!(self, Self::User | Self::Bot) {
            51
        } else {
            42
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refusal {
    None,
    CancelAfter,
    AccountBefore,
    Denied,
    ExternalAfter,
    StaleBefore,
    Restart,
    RecoveryV11,
    ConfirmedCleanupV12,
}
impl Refusal {
    fn restart(self) -> bool {
        matches!(self, Self::Restart | Self::RecoveryV11)
    }
    fn canceled_write(self) -> bool {
        self == Self::CancelAfter || self.restart()
    }
}
fn graph(query: &str, id: &str, node: Value) -> GhStep {
    write_graph(
        json!({"query":query,"variables":{"id":id}}),
        json!({"data":{"node":node}}),
    )
}
fn write_graph(payload: Value, response: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &response.to_string(),
    );
    step.stdin = serde_json::to_vec(&payload).unwrap();
    step
}
fn operation_row(h: &history::GitImagesV1, operation: Operation) -> Value {
    let mut value = row(
        h,
        if operation.author() == 51 {
            Effect::Dismiss
        } else {
            Effect::Summary
        },
        Failure::None,
        false,
    );
    if operation == Operation::LastDelete {
        value["body"] = json!("");
    }
    if operation == Operation::Bot {
        value["user"]["type"] = json!("Bot");
        value["user"]["login"] = json!("actual-review-bot[bot]");
    }
    value
}
fn operation_comments(h: &history::GitImagesV1, operation: Operation) -> Value {
    let mut value = comments(
        h,
        if operation.author() == 51 {
            Effect::Dismiss
        } else {
            Effect::Summary
        },
        Failure::None,
        "LEFT",
    );
    if operation == Operation::Bot {
        value[0]["user"] = operation_row(h, operation)["user"].clone();
    }
    value
}
fn operation_snapshot(h: &history::GitImagesV1, operation: Operation) -> Vec<GhStep> {
    let value = operation_row(h, operation);
    vec![
        get(&format!("repos/{FIRST}/pulls/7/reviews/810"), value.clone()),
        get(
            &format!("repos/{FIRST}/pulls/7/reviews/810/comments?per_page=100&page=1"),
            operation_comments(h, operation),
        ),
        get(&format!("repos/{FIRST}/pulls/7/reviews/810"), value),
    ]
}
fn graph_parent(h: &history::GitImagesV1) -> Value {
    let actual = parent(h);
    let repo = repository(0);
    json!({"id":actual["node_id"],"fullDatabaseId":actual["id"],"number":7,"headRefOid":h.h,"baseRefOid":h.b,
        "repository":{"id":repo["node_id"],"databaseId":repo["id"],"nameWithOwner":FIRST}})
}
const PENDING_REVIEW:&str="query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id fullDatabaseId body state commit{oid} author{__typename login ... on User{databaseId} ... on Bot{databaseId}} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
const PENDING_COMMENT:&str="query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id fullDatabaseId body state path line startLine commit{oid} updatedAt author{__typename login ... on User{databaseId} ... on Bot{databaseId}} viewerCanUpdate viewerCanDelete pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
const REVIEWER:&str="query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id fullDatabaseId state submittedAt commit{oid} author{__typename login avatarUrl ... on User{id databaseId name} ... on Bot{id databaseId}} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
const TEAM:&str="query($id:ID!){node(id:$id){__typename ... on Team{id databaseId slug name organization{id databaseId login}}}}";
const FIELDS:&str="__typename id fullDatabaseId body state path line startLine commit{oid} updatedAt author{__typename login ... on User{databaseId} ... on Bot{databaseId}} pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}";
fn review_node(h: &history::GitImagesV1, operation: Operation) -> Value {
    let raw = operation_row(h, operation);
    let parent = graph_parent(h);
    json!({"__typename":"PullRequestReview","id":"ACTUAL_NATIVE_REVIEW_810","fullDatabaseId":"810","body":raw["body"],"state":raw["state"],
        "submittedAt":raw["submitted_at"],"commit":{"oid":h.h},"author":{"__typename":raw["user"]["type"],"databaseId":operation.author(),
            "id":if operation==Operation::Bot{"ACTUAL_BOT_51"}else{"ACTUAL_USER_51"},"login":raw["user"]["login"],"name":null,"avatarUrl":null},
        "pullRequest":parent,"repository":parent["repository"]})
}
fn comment_node(h: &history::GitImagesV1) -> Value {
    let parent = graph_parent(h);
    json!({"__typename":"PullRequestReviewComment","id":"ACTUAL_NATIVE_COMMENT_911","fullDatabaseId":"911",
        "body":"Actual existing server pending comment","state":"PENDING","path":history::PATH,"line":2,"startLine":1,"commit":{"oid":h.h},
        "updatedAt":"2026-10-15T09:00:00Z","author":{"__typename":"User","databaseId":42,"login":"octocat"},
        "viewerCanUpdate":true,"viewerCanDelete":true,"pullRequestReview":{"id":"ACTUAL_NATIVE_REVIEW_810","fullDatabaseId":"810","state":"PENDING"},
        "pullRequest":parent,"repository":parent["repository"]})
}
fn teams() -> Value {
    json!([{"id":71,"node_id":"ACTUAL_TEAM_71","slug":"actual-reviewers","name":"Actual repository reviewers"}])
}
fn requested(operation: Operation, after: bool) -> Value {
    let mut result = json!({"users":[],"teams":[]});
    if !after {
        return result;
    }
    if operation == Operation::Team {
        result["teams"] = json!([{"id":71,"node_id":"ACTUAL_TEAM_71","slug":"actual-reviewers"}]);
    } else {
        result["users"] = json!([{"id":51,"node_id":if operation==Operation::Bot{"ACTUAL_BOT_51"}else{"ACTUAL_USER_51"},"type":if operation==Operation::Bot{"Bot"}else{"User"}}]);
    }
    result
}
fn observation(h: &history::GitImagesV1, operation: Operation) -> Vec<GhStep> {
    let mut steps = Vec::new();
    if operation == Operation::Team {
        steps.push(get(
            &format!("repos/{FIRST}/teams?per_page=100&page=1"),
            teams(),
        ));
        steps.push(graph(TEAM,"ACTUAL_TEAM_71",json!({"__typename":"Team","id":"ACTUAL_TEAM_71","databaseId":71,"slug":"actual-reviewers",
            "name":"Actual repository reviewers","organization":{"id":"ACTUAL_ORG_12","databaseId":12,"login":FIRST.split_once('/').unwrap().0}})));
    } else {
        steps.extend(operation_snapshot(h, operation));
        steps.push(graph(
            if operation.reviewer() {
                REVIEWER
            } else {
                PENDING_REVIEW
            },
            "ACTUAL_NATIVE_REVIEW_810",
            review_node(h, operation),
        ));
        if !operation.reviewer() && operation != Operation::Add {
            steps.push(graph(
                PENDING_COMMENT,
                "ACTUAL_NATIVE_COMMENT_911",
                comment_node(h),
            ));
        }
    }
    if operation.reviewer() {
        steps.push(get(
            &format!("repos/{FIRST}/pulls/7/requested_reviewers"),
            requested(operation, false),
        ));
    }
    steps
}
fn phase(h: &history::GitImagesV1, operation: Operation, images: &[GhStep]) -> Vec<GhStep> {
    let mut steps = opening(h);
    if operation == Operation::Add {
        steps.extend(images[3..images.len() - 3].iter().cloned());
    }
    steps.extend(observation(h, operation));
    steps.extend(closing(h));
    steps
}
fn mutation_v3(h: &history::GitImagesV1, operation: Operation) -> GhStep {
    let (input_type,method,input,fields,response)=match operation{
        Operation::User|Operation::Bot|Operation::Team=>{
            let(field,node)=match operation{Operation::User=>("userIds","ACTUAL_USER_51"),Operation::Bot=>("botIds","ACTUAL_BOT_51"),_=>("teamIds","ACTUAL_TEAM_71")};
            let mut input=json!({"pullRequestId":parent(h)["node_id"]});input[field]=json!([node]);
            ("RerequestReviewsInput","rerequestReviews",input,"pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}}".into(),
                json!({"pullRequest":graph_parent(h)}))
        },
        Operation::Delete|Operation::LastDelete=>("DeletePullRequestReviewCommentInput","deletePullRequestReviewComment",json!({"id":"ACTUAL_NATIVE_COMMENT_911"}),
            "pullRequestReview{id fullDatabaseId state} pullRequestReviewComment{id fullDatabaseId}".into(),
            json!({"pullRequestReview":if operation==Operation::LastDelete{Value::Null}else{json!({"id":"ACTUAL_NATIVE_REVIEW_810","fullDatabaseId":"810","state":"PENDING"})},
                "pullRequestReviewComment":{"id":"ACTUAL_NATIVE_COMMENT_911","fullDatabaseId":"911"}})),
        Operation::Edit=>{let mut value=comment_node(h);value["body"]=json!(BODY);
            ("UpdatePullRequestReviewCommentInput","updatePullRequestReviewComment",json!({"pullRequestReviewCommentId":"ACTUAL_NATIVE_COMMENT_911","body":BODY}),
                format!("pullRequestReviewComment{{{FIELDS}}}"),json!({"pullRequestReviewComment":value}))},
        Operation::Add=>{let mut value=comment_node(h);value["id"]=json!("ACTUAL_NATIVE_COMMENT_912");value["fullDatabaseId"]=json!("912");value["body"]=json!(BODY);
            ("AddPullRequestReviewThreadInput","addPullRequestReviewThread",json!({"pullRequestReviewId":"ACTUAL_NATIVE_REVIEW_810","path":history::PATH,
                "line":2,"side":"LEFT","startLine":1,"startSide":"LEFT","subjectType":"LINE","body":BODY}),
                format!("thread{{id comments(first:2){{nodes{{{FIELDS}}} totalCount pageInfo{{hasNextPage}}}}}}"),
                json!({"thread":{"id":"ACTUAL_NATIVE_PENDING_THREAD_912","comments":{"nodes":[value],"totalCount":1,"pageInfo":{"hasNextPage":false}}}}))},
    };
    write_graph(
        json!({"query":format!("mutation($input:{input_type}!){{{method}(input:$input){{{fields}}}}}"),"variables":{"input":input}}),
        json!({"data":{method:response}}),
    )
}
fn receipt(h: &history::GitImagesV1, operation: Operation) -> Vec<GhStep> {
    if operation.reviewer() {
        return vec![get(
            &format!("repos/{FIRST}/pulls/7/requested_reviewers"),
            requested(operation, true),
        )];
    }
    if operation == Operation::LastDelete {
        return vec![
            catalogue(json!([])),
            graph(
                "query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id}}}",
                "ACTUAL_NATIVE_REVIEW_810",
                Value::Null,
            ),
            graph(
                "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id}}}",
                "ACTUAL_NATIVE_COMMENT_911",
                Value::Null,
            ),
        ];
    }
    let mut comments = operation_comments(h, operation).as_array().unwrap().clone();
    match operation {
        Operation::Edit => comments[0]["body"] = json!(BODY),
        Operation::Delete => comments.clear(),
        Operation::Add => {
            let mut added = comments[0].clone();
            added["id"] = json!(912);
            added["node_id"] = json!("ACTUAL_NATIVE_COMMENT_912");
            added["body"] = json!(BODY);
            added["side"] = json!("LEFT");
            added["start_side"] = json!("LEFT");
            comments.push(added);
        }
        _ => {}
    }
    let row = operation_row(h, operation);
    let mut steps = vec![
        get(&format!("repos/{FIRST}/pulls/7/reviews/810"), row.clone()),
        get(
            &format!("repos/{FIRST}/pulls/7/reviews/810/comments?per_page=100&page=1"),
            json!(comments),
        ),
        get(&format!("repos/{FIRST}/pulls/7/reviews/810"), row),
    ];
    if operation == Operation::Delete {
        steps.push(graph(
            "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id}}}",
            "ACTUAL_NATIVE_COMMENT_911",
            Value::Null,
        ));
    }
    steps
}
fn control(session: &mut Session, mouse: bool, title: &str, label: &str, body: bool) {
    if mouse {
        let point = session.wait_for_owned_completed_action(title, label, &[]);
        session.click(point.0, point.1);
    } else {
        let controls = if body {
            vec![
                "[Preview]",
                "[External editor]",
                "[Confirm action]",
                "[Keep draft / Close]",
            ]
        } else {
            vec!["[Preview]", "[Confirm action]", "[Keep draft / Close]"]
        };
        session.focus_completed_owned_action_v1(
            title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &controls,
            &[],
        );
        session.send_key(b"\r");
    }
}
fn stable(fixture: &Fixture) -> Vec<String> {
    raw(fixture).prepare("SELECT json_array(host,login,resource,body) FROM drafts WHERE NOT(host=?1 AND login='user-id:42' AND resource LIKE 'review-operation-v3/%') ORDER BY host,login,resource").unwrap()
    .query_map([HOST],|row|row.get(0)).unwrap().map(Result::unwrap).collect()
}
fn actual_writes(rows: &[Value]) -> usize {
    rows.iter()
        .filter(|row| {
            let bytes = row["stdin_hex"].as_str().unwrap_or("");
            let decoded = (0..bytes.len())
                .step_by(2)
                .map(|index| u8::from_str_radix(&bytes[index..index + 2], 16).unwrap())
                .collect::<Vec<_>>();
            serde_json::from_slice::<Value>(&decoded)
                .ok()
                .and_then(|body| body["query"].as_str().map(str::to_owned))
                .is_some_and(|query| query.starts_with("mutation("))
        })
        .count()
}
/// Navigation hints only. Keep the ORIGINAL eight-second Open deadline and
/// send another genuine scroll input only after a newer COMPLETE owned detail
/// frame actually changes. Exact requested body remains separately verified
/// against the durable original draft/reference below.
fn history_detail_body_ready_v15(
    session: &mut Session,
    mouse: bool,
    after: usize,
    deadline: Instant,
) {
    const TITLE: &str = "Review operation history • inspect only";
    let mut after = after;
    let mut previous = None;
    loop {
        let goal = std::cell::Cell::new(false);
        let observed = std::cell::RefCell::new(String::new());
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("Original bounded history Open/scroll deadline ended");
        session.wait_for_new_cursorless_condition(after,|frame|{
            let text=frame.contents();
            let owned=text.contains(TITLE)&&text.contains("[Enter Inspect]")&&text.contains("[r Refresh]")&&text.contains("[Esc Back]")&&text.contains("INSPECT ONLY");
            if !owned||previous.as_ref().is_some_and(|old|old==&text)
                ||previous.is_none()&&!text.contains("Recorded attempt #"){return false;}
            goal.set(text.contains("Exact protected LOCAL requested text")&&text.contains(BODY));
            *observed.borrow_mut()=text;true
        },"NEW complete original history detail or actual viewport movement, never an old list/footer",remaining);
        if goal.get() {
            return;
        }
        previous = Some(observed.into_inner());
        after = session.completed_cursorless_end();
        if mouse {
            let footer = crate::support::completed_navigation_controls_v1::owned_action_position(
                session.completed_cursorless_screen().unwrap(),
                TITLE,
                "[Enter Inspect]",
                &[],
            )
            .expect(
                "Same complete history frame supplies the actual footer without a renewed wait",
            );
            let (_, top) = crate::support::locate_screen(
                session.completed_cursorless_screen().unwrap(),
                TITLE,
            )
            .expect("Same complete owned history border");
            // The text viewport starts four rows below this actual border.
            // Footer-relative coordinates hit blank space after recovery adds
            // its own controls; they also fail when the viewport is capped.
            let body_row = top.checked_add(4).expect("bounded owned body row");
            assert!(body_row < footer.1.saturating_sub(2));
            session.wheel(footer.0, body_row, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
    }
}
fn summary_after_restart(session: &mut Session, fixture: &Fixture, mouse: bool, previous: usize) {
    choose(session, mouse, "[: Commands]", b":");
    overlay_ready(
        session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    ready(session, &[TITLE, "[s Source]", "[Esc Close]"]);
    discover(session, mouse);
    session.wait_for_command_count(fixture, previous + 7);
    ready(session, &[TITLE, FIRST]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(fixture, previous + 18);
    ready(session, &[ROW_TITLE]);
    choose(session, mouse, "1 Inbox", b"1");
    choose(session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(fixture, previous + 31);
    ready(session, &[DETAIL_TITLE, "[F9 Review history]", "[b Back]"]);
}

fn journey_v3(mouse: bool, operation: Operation, refusal: Refusal) {
    journey_with_lineage_v12(mouse, operation, refusal, None)
}
fn journey_with_lineage_v12(
    mouse: bool,
    operation: Operation,
    refusal: Refusal,
    continuation: Option<action_lineage_v12::Scenario>,
) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let images = steps[steps.len() - 17..].to_vec();
    if operation != Operation::Add {
        steps.truncate(steps.len() - 17);
    }
    steps.extend(source_open(&h, json!([operation_row(&h, operation)])));
    if operation == Operation::Team {
        let mut team = opening(&h);
        team.push(get(
            &format!("repos/{FIRST}/teams?per_page=100&page=1"),
            teams(),
        ));
        team.extend(closing(&h));
        steps.extend(team);
    } else {
        let mut selected = opening(&h);
        selected.extend(operation_snapshot(&h, operation));
        selected.extend(closing(&h));
        steps.extend(selected);
    }
    // Opening an empty Add editor retains the actual chosen anchor but does
    // not fetch images until nonempty requested body is freshly previewed.
    let mut opener = opening(&h);
    opener.extend(observation(&h, operation));
    opener.extend(closing(&h));
    steps.extend(opener);
    steps.extend(phase(&h, operation, &images));
    let confirm_start = steps.len();
    let mut confirmation = opening(&h);
    if operation == Operation::Add {
        confirmation.extend(images[3..images.len() - 3].iter().cloned());
    }
    confirmation.extend(observation(&h, operation));
    confirmation.extend(principal(42));
    confirmation.push(get(&format!("repos/{FIRST}/pulls/7"), parent(&h)));
    confirmation.extend(observation(&h, operation));
    if operation == Operation::Add {
        confirmation.push(get(
            &format!("repos/{FIRST}/pulls/7/files?per_page=100&page=1"),
            json!([h.file()]),
        ));
    }
    confirmation.push(get(&format!("repos/{FIRST}/pulls/7"), parent(&h)));
    confirmation.extend(closing(&h));
    if refusal == Refusal::AccountBefore {
        *confirmation.last_mut().unwrap() = get("user", json!({"id":43,"login":"octocat"}));
    }
    if refusal == Refusal::StaleBefore {
        let at = confirmation
            .iter()
            .position(|step| step.args.get(1) == Some(&format!("repos/{FIRST}/pulls/7")))
            .unwrap();
        let mut raw = parent(&h);
        raw["head"]["sha"] = json!(h.m);
        confirmation[at] = get(&format!("repos/{FIRST}/pulls/7"), raw);
        confirmation.truncate(at + 1);
    }
    let write_index = confirm_start + confirmation.len();
    let mut held = None;
    if !matches!(refusal, Refusal::AccountBefore | Refusal::StaleBefore) {
        let mut mutation = mutation_v3(&h, operation);
        if refusal == Refusal::Denied {
            mutation.status = 1;
            mutation.stdout.clear();
            mutation.stderr = b"HTTP 403: actual pending operation denied".to_vec();
        }
        confirmation.push(mutation);
        if refusal != Refusal::Denied {
            let start = confirmation.len();
            confirmation.extend(receipt(&h, operation));
            if refusal.canceled_write() {
                confirmation[start].delay_ms = 6_000;
                confirmation.truncate(start + 1);
                held = Some(confirm_start + start);
            } else {
                confirmation.push(get("user", json!({"id":42,"login":"octocat"})));
                confirmation.push(get(&format!("repos/{FIRST}"), repository(0)));
                confirmation.push(get(&format!("repos/{FIRST}/pulls/7"), parent(&h)));
                if matches!(
                    refusal,
                    Refusal::ExternalAfter | Refusal::ConfirmedCleanupV12
                ) {
                    confirmation.last_mut().unwrap().delay_ms = 6_000;
                    held = Some(confirm_start + confirmation.len() - 1);
                } else {
                    confirmation.push(get("user", json!({"id":42,"login":"octocat"})));
                }
            }
        }
    }
    steps.extend(confirmation);
    let history_start = steps.len();
    if refusal.restart() {
        let mut restart = initial();
        bind_steps(&mut restart, &h);
        steps.extend(restart);
        for _ in 0..2 {
            let mut read = opening(&h);
            read.extend(closing(&h));
            steps.extend(read);
        }
        if refusal == Refusal::RecoveryV11 {
            let mut inspect = opening(&h);
            inspect.push(catalogue(json!([operation_row(&h, operation)])));
            inspect.extend(operation_snapshot(&h, operation));
            inspect.extend(closing(&h));
            steps.extend(inspect);
            let mut acknowledge = opening(&h);
            acknowledge.extend(closing(&h));
            steps.extend(acknowledge);
        }
    }
    let lineage_start = steps.len();
    if let Some(scenario) = continuation {
        steps.extend(action_lineage_v12::steps(&h, scenario));
    }
    fixture.scripted_gh(&steps);
    foreign_draft(&fixture);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(42)).unwrap(),
            "review-draft/original-batch-kept",
            "Original batch/comments/ref must survive selected operation autosave",
        )
        .unwrap();
    let original = stable(&fixture);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let git_calls = git_commands(&fixture);
    if operation == Operation::Add {
        choose(&mut session, mouse, "[O Old image]", b"O");
        overlay_ready(
            &mut session,
            "Immutable file images",
            &["[R Range]", "[V Batch]"],
        );
        selected_image_range(&mut session, mouse, "LEFT", &h.h);
        choose(&mut session, mouse, "[V Batch]", b"V");
    } else {
        choose(&mut session, mouse, "[F10 Reviews]", b"\x1b[21~");
    }
    overlay_ready(
        &mut session,
        "Scoped reviews",
        &["[R Resume selected]", "[F3 Edit pending]", "[F6 Teams]"],
    );
    // Preserve old geometry as a REAL resize-only surface, then prove every
    // full action becomes reachable at the new bounded useful minimum.
    session.resize_owned_single_action_modal_v2(40, 14, "Scoped reviews", "[Esc Close]");
    let before = commands(&session, &fixture).len();
    let sql_before = stable(&fixture);
    session.send_key(b"\x1bOR");
    ready(&mut session, &["Resize to inspect reviews"]);
    assert_eq!(commands(&session, &fixture).len(), before);
    assert_eq!(stable(&fixture), sql_before);
    session.resize_owned_control_block_v3(40, 18, "Scoped reviews", &REVIEW_CONTROLS_V12);
    overlay_ready(
        &mut session,
        "Scoped reviews",
        &["[F3 Edit pending]", "[F7 Re-request team]", "[Q Quit]"],
    );
    session.resize_owned_control_block_v3(120, 40, "Scoped reviews", &REVIEW_CONTROLS_V12);
    overlay_ready(
        &mut session,
        "Scoped reviews",
        &["[R Resume selected]", "[F6 Teams]"],
    );
    if !operation.reviewer() {
        let before = commands(&session, &fixture).len();
        let sql_before = stable(&fixture);
        choose(&mut session, mouse, "[B New batch]", b"B");
        ready(
            &mut session,
            &["Own PENDING exists; Resume or G Add range. New batch unavailable."],
        );
        assert_eq!(
            commands(&session, &fixture).len(),
            before,
            "existing PENDING reason must not queue transport"
        );
        assert_eq!(
            stable(&fixture),
            sql_before,
            "disabled new batch preserves all original drafts/journal/cache"
        );
    }
    if operation == Operation::Team {
        choose(&mut session, mouse, "[F6 Teams]", b"\x1b[17~");
        ready(&mut session, &["actual-reviewers", "[F7 Re-request team]"]);
    } else {
        let before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[R Resume selected]", b"R");
        browser_ready(
            &mut session,
            before,
            "Actual existing server pending comment",
        );
    }
    let (label, key) = match operation {
        Operation::Edit => ("[F3 Edit pending]", &b"\x1bOR"[..]),
        Operation::Delete | Operation::LastDelete => ("[F4 Delete pending]", &b"\x1bOS"[..]),
        Operation::Add => ("[G Add selected range]", &b"G"[..]),
        Operation::Team => ("[F7 Re-request team]", &b"\x1b[18~"[..]),
        _ => ("[F5 Re-request reviewer]", &b"\x1b[15~"[..]),
    };
    choose(&mut session, mouse, label, key);
    let title = format!("{} • {FIRST}#7", operation.label());
    ready(
        &mut session,
        &[operation.label(), "[Preview]", "[Confirm action]"],
    );
    if operation.body() {
        if mouse {
            session.click_owned_form_field(
                "PR INTENT • Preview first",
                "Exact server PENDING comment",
            );
        }
        session.send_key(b"\x01");
        session.paste(BODY);
        ready(&mut session, &[BODY]);
    }
    let before = session.completed_cursorless_end();
    control(&mut session, mouse, &title, "[Preview]", operation.body());
    session.wait_for_new_cursorless_condition(
        before,
        |screen| {
            let text = screen.contents();
            text.contains("READ-ONLY PREVIEW")
                && text.contains("[Confirm action]")
                && text.contains(&h.h)
        },
        "NEW actual typed original pending operation Preview/current H and complete Confirm",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&session, &fixture).len(), confirm_start);
    let before = session.completed_cursorless_end();
    session.paste("V9_PREPARED_POISON_MUST_NOT_CHANGE_INTENT");
    session.wait_for_new_cursorless_condition(
        before,
        |screen| {
            screen.contents().contains("READ-ONLY PREVIEW")
                && !screen
                    .contents()
                    .contains("V9_PREPARED_POISON_MUST_NOT_CHANGE_INTENT")
        },
        "prepared pending operation preview is read-only under paste",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&session, &fixture).len(), confirm_start);
    let outcome = session.completed_cursorless_end();
    control(
        &mut session,
        mouse,
        &title,
        "[Confirm action]",
        operation.body(),
    );
    let mut protected = None;
    if let Some(ordinal) = held {
        session.wait_for_command_count(&fixture, ordinal + 1);
        let journal = commands(&session, &fixture);
        let pid = i32::try_from(journal[ordinal]["pid"].as_u64().unwrap()).unwrap();
        let owned = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
        let identity = process_identity(pid).unwrap().unwrap();
        let mut hold = HeldOriginal::stop(identity, &owned);
        let saved:(String,String)=raw(&fixture).query_row("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-operation-v3/%'",[HOST],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        let encoded: Value = serde_json::from_str(&saved.1).unwrap();
        assert_eq!(encoded["body"], if operation.body() { BODY } else { "" });
        assert!(encoded["publication"]["attempt_id"]
            .as_i64()
            .is_some_and(|id| id > 0));
        protected = Some(saved.clone());
        if refusal == Refusal::ConfirmedCleanupV12 {
            let db = rusqlite::Connection::open_with_flags(
                database(&fixture),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap();
            db.execute_batch(&format!("CREATE TABLE deliberate_v12_cleanup_hits(attempt_id INTEGER NOT NULL, resource TEXT NOT NULL); CREATE TRIGGER deliberate_v12_cleanup_failure BEFORE DELETE ON drafts WHEN OLD.host='{HOST}' AND OLD.login='user-id:42' AND OLD.resource='{}' BEGIN INSERT INTO deliberate_v12_cleanup_hits(attempt_id,resource) SELECT json_extract(OLD.body,'$.publication.attempt_id'),OLD.resource; SELECT RAISE(FAIL,'DELIBERATE_V12_LOCAL_CLEANUP_SQL_FAILURE'); END;",saved.0)).unwrap();
            let reference = &encoded["publication"];
            let confirmed: (String, String) = db
                .query_row(
                    "SELECT state,confirmed_json FROM pr_write_attempts WHERE attempt_id=?1",
                    [reference["attempt_id"].as_i64().unwrap()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(confirmed.0, "confirmed");
            let effect: Value = serde_json::from_str(&confirmed.1).unwrap();
            assert_eq!(effect["resource_id"], "911");
            assert_eq!(effect["node_id"], "ACTUAL_NATIVE_COMMENT_911");
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM deliberate_v12_cleanup_hits",
                    [],
                    |row| row.get::<_, usize>(0)
                )
                .unwrap(),
                0
            );
            hold.resume();
        } else if refusal == Refusal::ExternalAfter {
            let db = rusqlite::Connection::open_with_flags(
                database(&fixture),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap();
            assert_eq!(db.execute("UPDATE drafts SET body=?1 WHERE host=?2 AND login='user-id:42' AND resource=?3",rusqlite::params!["EXTERNAL newer pending operation draft",HOST,saved.0]).unwrap(),1);
            drop(db);
            hold.resume();
        } else {
            choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
            session.wait_for_new_cursorless_condition(
                outcome,
                |screen| {
                    let text = screen.contents();
                    text.contains("canceled")
                        && crate::support::main_footer_visible(screen)
                        && !text.contains("PR INTENT")
                        && !text.contains("READ-ONLY PREVIEW")
                        && !text.contains("Scoped reviews")
                        && !text.contains(BODY)
                        && !text.contains("Actual existing server pending comment")
                },
                "Canceled invoked pending write hides private source and never resends",
                Duration::from_secs(8),
            );
        }
    }
    if !refusal.canceled_write() {
        session.wait_for_new_cursorless_condition(
            outcome,
            |screen| {
                let text = screen.contents();
                let status = match refusal {
                    Refusal::None => {
                        if operation.reviewer() {
                            "delivery unknown"
                        } else if operation == Operation::LastDelete {
                            "absence observed"
                        } else {
                            "confirmed"
                        }
                    }
                    Refusal::AccountBefore => "Not sent",
                    Refusal::StaleBefore => "Original review source unavailable",
                    Refusal::Denied => "Outcome unknown",
                    Refusal::ExternalAfter => "source unavailable",
                    Refusal::ConfirmedCleanupV12 => "confirmed • source unavailable",
                    _ => unreachable!(),
                };
                let (rows, cols) = screen.size();
                let actual_status = screen
                    .rows(0, cols)
                    .nth(usize::from(rows.saturating_sub(2)))
                    .unwrap_or_default();
                crate::support::main_footer_visible(screen)
                    && actual_status.contains(status)
                    && !text.contains("PR INTENT")
                    && !text.contains("READ-ONLY PREVIEW")
                    && !text.contains("Scoped reviews")
                    && !text.contains(BODY)
                    && !text.contains("Actual existing server pending comment")
            },
            "Actual typed safe-only pending effect/refusal and original private retirement",
            Duration::from_secs(8),
        );
    }
    session.wait_for_command_count(&fixture, history_start);
    let executed = commands(&session, &fixture);
    let before_restart = executed.len();
    assert_eq!(
        actual_writes(&executed),
        usize::from(!matches!(
            refusal,
            Refusal::AccountBefore | Refusal::StaleBefore
        ))
    );
    for (index, actual) in executed.iter().enumerate() {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(steps[index].args));
        assert_eq!(
            actual["stdin_hex"],
            steps[index]
                .stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
    let count = raw(&fixture)
        .query_row(
            "SELECT count(*) FROM pr_write_attempts WHERE command=?1",
            [operation.command()],
            |row| row.get::<_, usize>(0),
        )
        .unwrap();
    assert_eq!(count, usize::from(refusal != Refusal::StaleBefore));
    if refusal != Refusal::StaleBefore {
        let state: String = raw(&fixture)
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE command=?1",
                [operation.command()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            state,
            if refusal.canceled_write() || refusal == Refusal::Denied {
                "uncertain"
            } else if refusal == Refusal::AccountBefore {
                "pending"
            } else {
                "confirmed"
            }
        );
    }
    assert_eq!(
        raw(&fixture)
            .query_row(
                "SELECT count(*) FROM pr_write_review_operation_origins_v3",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
        usize::from(refusal != Refusal::StaleBefore)
    );
    if refusal == Refusal::ExternalAfter {
        let saved = protected.as_ref().unwrap();
        assert_eq!(
            raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource=?1",
                    [&saved.0],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "EXTERNAL newer pending operation draft"
        );
    } else if let Some(saved) = &protected {
        assert_eq!(
            raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource=?1",
                    [&saved.0],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            saved.1
        );
    }
    assert_eq!(stable(&fixture), original);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), git_calls);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    if refusal == Refusal::ConfirmedCleanupV12 {
        let saved = protected.as_ref().unwrap();
        let reference: Value = serde_json::from_str(&saved.1).unwrap();
        let expected_attempt = reference["publication"]["attempt_id"].as_i64().unwrap();
        let db = rusqlite::Connection::open_with_flags(
            database(&fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        assert_eq!(
            db.query_row(
                "SELECT attempt_id,resource FROM deliberate_v12_cleanup_hits",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            )
            .unwrap(),
            (expected_attempt, saved.0.clone())
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM deliberate_v12_cleanup_hits",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
            1,
            "Actual conditional DELETE reached the deliberate SQL FAIL exactly once"
        );
        let before_marker: String = db.query_row("SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts WHERE attempt_id=?1",[expected_attempt],|row|row.get(0)).unwrap();
        let before_origin: String = db
            .query_row(
                "SELECT origin_json FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",
                [expected_attempt],
                |row| row.get(0),
            )
            .unwrap();
        db.execute_batch("DROP TRIGGER deliberate_v12_cleanup_failure")
            .unwrap();
        drop(db);
        action_lineage_v12::continue_after_ack(
            &fixture,
            &h,
            mouse,
            continuation.unwrap(),
            lineage_start,
            &steps,
            saved,
            &original,
            &git,
            &config,
        );
        let db = raw(&fixture);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM deliberate_v12_cleanup_hits",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [expected_attempt],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "confirmed",
            "A genuine confirmed terminal basis is NEVER changed to local ACK"
        );
        assert_eq!(db.query_row("SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts WHERE attempt_id=?1",[expected_attempt],|row|row.get::<_,String>(0)).unwrap(),before_marker,"The old actual journal row/receipt/reference is untouched by explicit New");
        assert_eq!(
            db.query_row(
                "SELECT origin_json FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",
                [expected_attempt],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            before_origin
        );
        return;
    }
    if refusal.restart() {
        let mut session = launch(&fixture, mouse);
        summary_after_restart(&mut session, &fixture, mouse, history_start);
        choose(&mut session, mouse, "[F9 Review history]", b"\x1b[20~");
        overlay_ready(
            &mut session,
            "Review operation history • inspect only",
            &["[Enter Inspect]", "[r Refresh]", "[Esc Back]"],
        );
        let detail_after = session.completed_cursorless_end();
        let detail_deadline = Instant::now() + Duration::from_secs(8);
        choose(&mut session, mouse, "[Enter Inspect]", b"\r");
        history_detail_body_ready_v15(&mut session, mouse, detail_after, detail_deadline);
        assert_eq!(actual_writes(&commands(&session, &fixture)), 1);
        assert_eq!(stable(&fixture), original);
        assert_eq!(git_state(&fixture), git);
        if refusal == Refusal::RecoveryV11 {
            let count = commands(&session, &fixture).len();
            let sql_before = stable(&fixture);
            choose(&mut session, mouse, "[a Acknowledge local]", b"a");
            ready(&mut session, &["Fresh explicit Inspect current required"]);
            assert_eq!(
                commands(&session, &fixture).len(),
                count,
                "historical row/ref alone never authorizes ACK"
            );
            assert_eq!(stable(&fixture), sql_before);
            let before = session.completed_cursorless_end();
            choose(&mut session, mouse, "[i Inspect current]", b"i");
            session.wait_for_new_cursorless_condition(before,|frame|{
                let text=frame.contents();text.contains("Review operation history • inspect only")&&text.contains("FRESH CURRENT OUTCOME")
                    &&text.contains("[a Acknowledge local]")&&!text.contains("Original history read pending")
            },"Genuine fresh original-session current review/comment observation, not restored history approval",Duration::from_secs(8));
            assert_eq!(
                actual_writes(&commands(&session, &fixture)),
                1,
                "fresh inspection performs only actual declared reads"
            );
            assert_eq!(stable(&fixture), sql_before);
            let before = session.completed_cursorless_end();
            choose(&mut session, mouse, "[a Acknowledge local]", b"a");
            session.wait_for_new_cursorless_condition(before,|frame|{
                let text=frame.contents();crate::support::main_footer_visible(frame)&&text.contains("locally acknowledged")
                    &&text.contains("nothing resent")&&!text.contains("Review operation history")&&!text.contains(BODY)
                    &&!text.contains("Actual existing server pending comment")
            },"Explicit same-source local ACK releases only exact old blocker and retires original private presentation",Duration::from_secs(8));
            assert_eq!(
                actual_writes(&commands(&session, &fixture)),
                1,
                "ACK never resends original mutation"
            );
            assert_eq!(
                raw(&fixture)
                    .query_row(
                        "SELECT state FROM pr_write_attempts WHERE command=?1",
                        [operation.command()],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                "acknowledged"
            );
            assert_eq!(raw(&fixture).query_row("SELECT count(*) FROM pr_write_attempts WHERE state IN('pending','uncertain')",[],|row|row.get::<_,usize>(0)).unwrap(),0);
            let saved = protected.as_ref().unwrap();
            assert_eq!(
                raw(&fixture)
                    .query_row(
                        "SELECT body FROM drafts WHERE resource=?1",
                        [&saved.0],
                        |row| row.get::<_, String>(0)
                    )
                    .unwrap(),
                saved.1
            );
            assert_eq!(stable(&fixture), original);
            assert_eq!(git_state(&fixture), git);
            assert_eq!(
                fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
                config
            );
            assert_eq!(commands(&session, &fixture).len(), lineage_start);
            session.quit_and_assert_restored_terminal(mouse);
            if let Some(scenario) = continuation {
                action_lineage_v12::continue_after_ack(
                    &fixture,
                    &h,
                    mouse,
                    scenario,
                    lineage_start,
                    &steps,
                    protected.as_ref().unwrap(),
                    &original,
                    &git,
                    &config,
                );
            }
            return;
        }
        let detail_end = session.completed_cursorless_end();
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        session.wait_for_new_cursorless_condition(detail_end,|frame|{
            let text=frame.contents();text.contains("Review operation history • inspect only")
                &&text.contains("[Enter Inspect]")&&text.contains("[r Refresh]")&&text.contains("[Esc Back]")
                &&text.contains(operation.command())&&!text.contains("Exact protected LOCAL requested text")&&!text.contains(BODY)
        },"NEW completed original history LIST after detail Back before the second Back (no Alt-Esc or stale detail footer)",Duration::from_secs(8));
        let list_end = session.completed_cursorless_end();
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        session.wait_for_new_cursorless_condition(
            list_end,
            |frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && !text.contains("Review operation history • inspect only")
                    && !text.contains("Exact protected LOCAL requested text")
                    && !text.contains(BODY)
                    && text.contains(DETAIL_TITLE)
            },
            "NEW completed original Summary after history Close before the single Quit",
            Duration::from_secs(8),
        );
        session.quit_and_assert_restored_terminal(mouse);
        assert_eq!(commands(&session, &fixture).len(), steps.len());
        assert!(steps.len() > before_restart);
    }
    let _ = write_index;
}

macro_rules! pair_v3 {
    ($key:ident,$mouse:ident,$operation:ident,$failure:ident) => {
        #[test]
        fn $key() {
            journey_v3(false, Operation::$operation, Refusal::$failure);
        }
        #[test]
        fn $mouse() {
            journey_v3(true, Operation::$operation, Refusal::$failure);
        }
    };
}
pair_v3!(
    pending_reviews_v3_keyboard_edit,
    pending_reviews_v3_mouse_edit,
    Edit,
    None
);
pair_v3!(
    pending_reviews_v3_keyboard_delete,
    pending_reviews_v3_mouse_delete,
    Delete,
    None
);
pair_v3!(
    pending_reviews_v3_keyboard_last_delete,
    pending_reviews_v3_mouse_last_delete,
    LastDelete,
    None
);
pair_v3!(
    pending_reviews_v3_keyboard_add_original_range,
    pending_reviews_v3_mouse_add_original_range,
    Add,
    None
);
pair_v3!(
    pending_reviews_v3_keyboard_rerequest_user,
    pending_reviews_v3_mouse_rerequest_user,
    User,
    None
);
pair_v3!(
    pending_reviews_v3_keyboard_rerequest_bot,
    pending_reviews_v3_mouse_rerequest_bot,
    Bot,
    None
);
pair_v3!(
    pending_reviews_v3_keyboard_rerequest_team,
    pending_reviews_v3_mouse_rerequest_team,
    Team,
    None
);
pair_v3!(
    pending_reviews_v3_keyboard_invoked_cancel,
    pending_reviews_v3_mouse_invoked_cancel,
    Edit,
    CancelAfter
);
pair_v3!(
    pending_reviews_v3_keyboard_changed_uid,
    pending_reviews_v3_mouse_changed_uid,
    Edit,
    AccountBefore
);
pair_v3!(
    pending_reviews_v3_keyboard_denied,
    pending_reviews_v3_mouse_denied,
    Delete,
    Denied
);
pair_v3!(
    pending_reviews_v3_keyboard_external_sql,
    pending_reviews_v3_mouse_external_sql,
    Edit,
    ExternalAfter
);
pair_v3!(
    pending_reviews_v3_keyboard_stale_head,
    pending_reviews_v3_mouse_stale_head,
    Edit,
    StaleBefore
);
pair_v3!(
    pending_reviews_v3_keyboard_restart_history,
    pending_reviews_v3_mouse_restart_history,
    Edit,
    Restart
);
pair_v3!(
    pending_reviews_v11_keyboard_uncertain_restart_fresh_inspection_explicit_local_ack,
    pending_reviews_v11_mouse_uncertain_restart_fresh_inspection_explicit_local_ack,
    Edit,
    RecoveryV11
);
