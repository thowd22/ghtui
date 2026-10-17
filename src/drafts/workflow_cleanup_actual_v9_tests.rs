//! Actual closed Catalogue/Definition/Form/local Git/TLS Push producers, then
//! the SAME controlled publication queue. No test-created seal or live proof.
use super::*;
use crate::{
    actions_workspace::{ActionsAction, ActionsView},
    active_remote_scope_v1::ActiveRemoteScopeV1,
    app::{App, View},
    forms::FormAction,
    providers::gh::*,
    repository_catalogue_intent_v1::IntentReference,
    repository_catalogue_save_v1::load_catalogue_intent_source,
    scoped_actions_native_v1::{self as native, publication_v3 as publication},
    scoped_actions_publication_test_support_v3 as w, scoped_actions_resources_test_support_v2 as r,
    scoped_actions_test_support_v1 as f,
    scoped_actions_v1::{initial, prepare_initial_read},
    test_support::{Fixture, GhStep},
    workflow_tls_contracts_v9::workflow_tls_v9::WorkflowTlsRemoteV9,
};
use rusqlite::{types::Value as SqlValue, Connection, OpenFlags};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    thread,
};

const HOST: &str = "ghe.example";
const REPOSITORY: &str = "example/ci";
const BRANCH: &str = "workflow/edit";
const ORIGINAL: &str = "on: push\njobs: {}\n";
const EDITED: &str = "on: push\njobs: {}\n# PRIVATE_ACTUAL_CLEANUP_EDIT\n";
type SqlImage = Vec<(String, Vec<Vec<SqlValue>>)>;

// Only this actual publisher test thread may pause its own first receive.
// Normal builds contain neither this slot nor the callsite. No test callback,
// source/ACK/seal construction or synthetic queue result is involved.
struct ReceiveGateV9 {
    reached: SyncSender<()>,
    release: std::sync::Mutex<Receiver<()>>,
    used: std::sync::atomic::AtomicBool,
}
thread_local! {
    static RECEIVE_GATE_V9: std::cell::RefCell<Option<Arc<ReceiveGateV9>>> = const { std::cell::RefCell::new(None) };
}
struct ReceiveRegistrationV9(Arc<ReceiveGateV9>);
impl Drop for ReceiveRegistrationV9 {
    fn drop(&mut self) {
        RECEIVE_GATE_V9.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|gate| Arc::ptr_eq(gate, &self.0)) {
                slot.take();
            }
        });
    }
}
fn install_receive_gate_v9(gate: Arc<ReceiveGateV9>) -> ReceiveRegistrationV9 {
    RECEIVE_GATE_V9.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(gate.clone());
    });
    ReceiveRegistrationV9(gate)
}
pub(super) fn before_actual_receive_v9() {
    let gate = RECEIVE_GATE_V9.with(|slot| slot.borrow().clone());
    if let Some(gate) = gate {
        if !gate.used.swap(true, std::sync::atomic::Ordering::AcqRel) {
            gate.reached.try_send(()).unwrap();
            gate.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(8))
                .expect("Bounded actual test waiter release");
        }
    }
}

fn converted(mut step: GhStep) -> GhStep {
    let convert = |value: &str| value.replace(f::HOST, HOST).replace(f::REPO, REPOSITORY);
    for arg in &mut step.args {
        *arg = convert(arg);
    }
    step.stdout = convert(std::str::from_utf8(&step.stdout).unwrap()).into_bytes();
    step.stderr = convert(std::str::from_utf8(&step.stderr).unwrap()).into_bytes();
    step
}
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &["api", endpoint, "--hostname", HOST, "--method", "GET"],
        &value.to_string(),
    )
}
fn repository() -> Value {
    let mut value = f::repo();
    value["full_name"] = json!(REPOSITORY);
    value["permissions"]["push"] = json!(true);
    value["clone_url"] = json!(format!("https://{HOST}/{REPOSITORY}.git"));
    value["ssh_url"] = json!(format!("git@{HOST}:{REPOSITORY}.git"));
    value
}
fn phase(confirm: bool) -> Vec<GhStep> {
    let workflow = || {
        f::get(
            &format!("repos/{}/actions/workflows/7", f::REPO),
            r::workflow(),
        )
    };
    let mut result = r::wrap(vec![workflow(), workflow()], 42);
    if confirm {
        result.push(f::get("user", json!({"id":42,"login":"octocat"})));
    }
    result.into_iter().map(converted).collect()
}
fn definition(c: &str) -> Vec<GhStep> {
    let mut result = r::definition(r::workflow());
    for step in &mut result {
        if step.args.iter().any(|arg| arg.ends_with("/commits/main")) {
            step.stdout = json!({"sha":c}).to_string().into_bytes();
        }
        if let Some(endpoint) = step
            .args
            .iter_mut()
            .find(|arg| arg.contains("/contents/.github/workflows/ci.yml?ref="))
        {
            *endpoint = format!(
                "repos/{}/contents/.github/workflows/ci.yml?ref={c}",
                f::REPO
            );
            step.stdout = ORIGINAL.as_bytes().to_vec();
        }
    }
    result.into_iter().map(converted).collect()
}
fn remote_phase(c: &str, d: &str, confirm: bool) -> Vec<GhStep> {
    let principal = || {
        f::principal(42)
            .into_iter()
            .map(converted)
            .collect::<Vec<_>>()
    };
    let workflow = || {
        get(
            &format!("repos/{REPOSITORY}/actions/workflows/7"),
            r::workflow(),
        )
    };
    let base = || {
        get(
            &format!("repos/{REPOSITORY}/git/ref/heads/main"),
            json!({"ref":"refs/heads/main","object":{"type":"commit","sha":c}}),
        )
    };
    let absent = || {
        let mut step = GhStep::read(
            &[
                "api",
                &format!("repos/{REPOSITORY}/git/ref/heads/workflow%2Fedit"),
                "--hostname",
                HOST,
                "--method",
                "GET",
                "--include",
            ],
            "HTTP/2 404 Not Found\nContent-Length: 23\r\n\r\n{\"message\":\"Not Found\"}",
        );
        step.status = 1;
        step.stderr = b"gh: Not Found (HTTP 404)\n".to_vec();
        step
    };
    let mut result = principal();
    result.push(get(&format!("repos/{REPOSITORY}"), repository()));
    result.push(workflow());
    result.push(get("repositories/10000", repository()));
    result.push(base());
    result.push(absent());
    result.push(base());
    result.push(absent());
    result.push(get("repositories/10000", repository()));
    result.push(workflow());
    result.push(get(&format!("repos/{REPOSITORY}"), repository()));
    result.extend(principal());
    if confirm {
        result.push(get(
            &format!("repos/{REPOSITORY}/git/ref/heads/workflow%2Fedit"),
            json!({"ref":format!("refs/heads/{BRANCH}"),"object":{"type":"commit","sha":d}}),
        ));
        result.push(get("user", json!({"id":42,"login":"octocat"})));
    }
    assert_eq!(result.len(), if confirm { 16 } else { 14 });
    result
}
fn exact(fixture: &Fixture, steps: &[GhStep]) {
    let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
        assert!(!step.args.iter().any(|arg| matches!(
            arg.as_str(),
            "POST" | "PUT" | "PATCH" | "DELETE" | "graphql"
        )));
    }
}
fn sql(path: &Path) -> SqlImage {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut out = Vec::new();
    for name in names {
        let mut query = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let count = query.column_count();
        let mut rows = query
            .query_map([], |r| {
                (0..count)
                    .map(|i| r.get::<_, SqlValue>(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| format!("{row:?}"));
        out.push((name, rows));
    }
    out
}
fn without_draft(mut rows: SqlImage, before: &[SqlValue]) -> SqlImage {
    let drafts = &mut rows
        .iter_mut()
        .find(|(name, _)| name == "drafts")
        .unwrap()
        .1;
    let count = drafts.len();
    drafts.retain(|row| row != before);
    assert_eq!(drafts.len() + 1, count);
    rows
}
fn controlled(path: &Path) -> (DraftWriter, Receiver<Message>) {
    let (sender, receiver) = mpsc::sync_channel(8);
    let (_, acks) = mpsc::channel();
    (
        DraftWriter {
            sender,
            acks,
            worker: None,
            pending: BTreeMap::new(),
            producer: Arc::new(DraftProducerV4::new(path.into())),
        },
        receiver,
    )
}
fn next(receiver: &Receiver<Message>) -> Message {
    receiver
        .recv_timeout(Duration::from_secs(8))
        .expect("Actual existing writer queue message")
}
fn dispatch(writer: &DraftWriter, message: Message, cache: &Result<Cache, String>) {
    writer.producer.publication_capacity_v2();
    let Message::PublicationV2(message) = message else {
        panic!("Actual closed publication message required")
    };
    message.run(cache);
}
struct Actual {
    fixture: Fixture,
    provider: GithubProvider,
    app: App,
    steps: Vec<GhStep>,
    tls: WorkflowTlsRemoteV9,
    writer: DraftWriter,
    receiver: Receiver<Message>,
    committed: String,
    original: String,
}
fn install_step(actual: &mut Actual, action: FormAction) {
    let work = w::request(&mut actual.app, action);
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    actual.app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = publication::execute(
        &owner,
        &actual.provider,
        &f::database(&actual.fixture),
        Some(&actual.writer.client()),
    )
    .unwrap();
    assert!(publication::publish(&mut actual.app, &owner, result));
    actual.app.scoped_actions_owner_v1 = None;
    w::paint(&mut actual.app, 120, 40);
}
fn actual() -> Actual {
    use crate::{
        git::RawPath,
        search::{
            corpus::LocalSource,
            managed::{CorpusOwnership, CorpusRepository},
        },
    };
    let fixture = Fixture::new();
    f::git(&fixture, &["init", "-b", "main"]);
    f::git(&fixture, &["config", "user.name", "Cleanup fixture"]);
    f::git(
        &fixture,
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::create_dir_all(fixture.repo().join(".github/workflows")).unwrap();
    for p in [
        fixture.repo(),
        fixture.repo().join(".github"),
        fixture.repo().join(".github/workflows"),
    ] {
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(fixture.repo().join("tracked"), b"original unrelated file\n").unwrap();
    fs::write(fixture.repo().join(".github/workflows/ci.yml"), ORIGINAL).unwrap();
    f::git(
        &fixture,
        &["add", "--", "tracked", ".github/workflows/ci.yml"],
    );
    f::git(&fixture, &["commit", "-m", "Original actual cleanup C"]);
    let c = f::git(&fixture, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        fixture.repo().join("tracked"),
        b"EXACT_UNRELATED_DIRTY_CANARY\n",
    )
    .unwrap();
    // Real URL-specific TLS/proxy configuration precedes EVERY actual source,
    // local mapping, root/config witness and Preview capture.
    let tls = WorkflowTlsRemoteV9::start(&fixture.root, &fixture.repo()).unwrap();
    tls.configure_fixture_repository(&fixture.repo(), "workflow-head")
        .unwrap();
    let mut steps = f::initial_steps()
        .into_iter()
        .map(converted)
        .collect::<Vec<_>>();
    steps.extend(r::workflows().into_iter().map(converted));
    steps.extend(definition(&c));
    steps.extend(definition(&c));
    for _ in 0..3 {
        steps.extend(phase(false));
        steps.extend(phase(true));
    }
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new(HOST)
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let path = f::database(&fixture);
    let mut cache = Cache::open(&path).unwrap();
    let scope = Scope::github(HOST, Some(42)).unwrap();
    cache
        .put_draft(&scope, "unrelated-cleanup-canary", "EXACT_UNRELATED_DRAFT")
        .unwrap();
    let source = Arc::new(
        load_catalogue_intent_source(
            &provider,
            &mut cache,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &Default::default(),
            &CancellationToken::default(),
        )
        .unwrap(),
    );
    let row = &source.source().repositories()[0];
    let reference = IntentReference {
        id: row.id,
        node_id: row.node_id.clone(),
        last_name: row.full_name.clone(),
    };
    let token = CancellationToken::default();
    let mut session = provider
        .begin_repository_catalogue_access(source.source(), &Default::default(), &token)
        .unwrap();
    session.verify(&reference, &token).unwrap();
    let window = session
        .reserve_external_reads(1, 1024 * 1024, &token)
        .unwrap();
    let bounded = provider
        .clone()
        .with_scoped_read_window(window.deadline(), window.max_output_bytes())
        .unwrap();
    let read = bounded
        .scoped_actions_catalogue_v1(
            session.observed_original_v1(reference.id).unwrap(),
            &Default::default(),
            &token,
        )
        .unwrap();
    let seed = prepare_initial_read(read, session.identity(), &token).unwrap();
    session.verify(&reference, &token).unwrap();
    let closed = session
        .close_with_guard(&token, || {
            cache
                .validate_repository_catalogue_intent_source(&source)
                .map_err(|_| GithubError {
                    kind: GithubErrorKind::IdentityChanged,
                    message: "Original actual fixture SQL changed".into(),
                })
        })
        .unwrap();
    let mut active =
        ActiveRemoteScopeV1::explicit_selection(source.source(), &[reference.id]).unwrap();
    active
        .observe_closed_access(source.source(), &closed)
        .unwrap();
    let initial = initial(
        source,
        Arc::new(active),
        reference,
        seed,
        &closed,
        &provider,
        &token,
    )
    .unwrap();
    let mut app = App::new();
    app.settings_host = HOST.into();
    app.view = View::Actions;
    native::install_initial(&mut app, initial);
    assert!(
        app.pr_check_continuation_request_v2.is_none()
            && app.pr_check_continuation_owner_v2.is_none()
            && app.pr_check_watch_v2.is_none()
    );
    app.pr_check_continuation_cancel_v2 = false;
    for action in [
        ActionsAction::View(ActionsView::Workflows),
        ActionsAction::OpenWorkflow {
            id: 7,
            revision: "main".into(),
        },
    ] {
        let result = f::read(&mut app, &provider, &fixture, action).unwrap();
        f::install(&mut app, result);
    }
    let mapping = CorpusRepository {
        scope: scope.clone(),
        repository: REPOSITORY.into(),
        directory: RawPath(fixture.repo().as_os_str().as_bytes().to_vec()),
        source: LocalSource::Revision(c.clone()),
        ownership: CorpusOwnership::Existing,
        active: true,
        saved_commit: Some(c.clone()),
        disk_bytes: 0,
        disk_complete: false,
        clone_source: None,
    };
    cache
        .save_view(
            &scope,
            &crate::search::managed::mapping_key(REPOSITORY),
            &serde_json::to_value(mapping).unwrap(),
        )
        .unwrap();
    let (writer, receiver) = controlled(&path);
    let client = writer.client();
    assert!(native::intercept(
        &mut app,
        &ActionsAction::EditWorkflowLocalV9
    ));
    let work = native::take_work(&mut app).unwrap();
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    publication::workflow_edit_v9::prepare_cutoff(&owner, &provider, &path, Some(&client)).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let opened = publication::execute(&owner, &provider, &path, Some(&client)).unwrap();
    assert!(publication::publish(&mut app, &owner, opened));
    app.scoped_actions_owner_v1 = None;
    let form = app.form.as_mut().unwrap();
    for (field, text) in form.fields.iter_mut().zip([
        "direct",
        EDITED,
        BRANCH,
        "Actual reviewed cleanup commit",
        "Actual private PR title",
        "Actual private PR body",
    ]) {
        field.editor = crate::editor::TextBuffer::new(text).unwrap();
    }
    form.invalidate_plan();
    w::paint(&mut app, 120, 40);
    let mut actual = Actual {
        fixture,
        provider,
        app,
        steps,
        tls,
        writer,
        receiver,
        committed: String::new(),
        original: c.clone(),
    };
    for _ in 0..3 {
        install_step(&mut actual, FormAction::Preview);
        install_step(&mut actual, FormAction::Submit);
    }
    actual.committed = f::git(&actual.fixture, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();
    assert_ne!(actual.committed, c);
    assert_eq!(
        f::git(&actual.fixture, &["show", "HEAD:.github/workflows/ci.yml"]),
        EDITED
    );
    let form = actual.app.form.as_mut().unwrap();
    assert_eq!(form.fields.len(), 9);
    assert_eq!(form.fields[6].choices, ["0"]);
    form.dispatch(&FormAction::Choice(6, 0));
    form.fields[7].editor = crate::editor::TextBuffer::new("workflow-head").unwrap();
    form.fields[8].editor = crate::editor::TextBuffer::new("main").unwrap();
    form.invalidate_plan();
    w::paint(&mut actual.app, 120, 40);
    actual
        .steps
        .extend(remote_phase(&c, &actual.committed, false));
    actual
        .steps
        .extend(remote_phase(&c, &actual.committed, true));
    actual.fixture.scripted_gh(&actual.steps);
    install_step(&mut actual, FormAction::Preview);
    actual
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    RawReceipt,
    LateNegativeCancel,
    LateNegativeDeadline,
    LostReply,
    LatePositiveCancel,
    LatePositiveDeadline,
    ForeignCache,
    ForeignAck,
}
fn actual_case(fault: Fault) {
    let mut actual = actual();
    let path = f::database(&actual.fixture);
    let cache = Ok(Cache::open(&path).unwrap());
    let client = actual.writer.client();
    let work = w::request(&mut actual.app, FormAction::Submit);
    let Some(publication::PublicationWorkV3::WorkflowEdit(publication)) =
        work.publication_v3.as_deref()
    else {
        panic!("Genuine actual workflow work")
    };
    let publication::workflow_edit_v9::WorkflowEditWorkV9::Form {
        confirm: Some(publication::workflow_edit_v9::WorkflowEditConfirmV9::Remote(confirm)),
        ..
    } = &**publication
    else {
        panic!("Actual worker-captured remote Confirm")
    };
    let confirm = confirm.clone();
    let token = confirm.cancellation().clone();
    let intent = confirm
        .preview()
        .selection()
        .committed()
        .staged()
        .saved()
        .opened()
        .intent();
    let mut fence = client
        .capture_workflow_edit_publication_v9(&path, intent, &token, confirm.deadline())
        .unwrap();
    if matches!(
        fault,
        Fault::LateNegativeDeadline | Fault::LatePositiveDeadline
    ) {
        // SAME actual fence narrows Core/pool/cursor before construction. The
        // original request D stays live; no existing phase clock is renewed.
        fence.deadline = fence.deadline.min(Instant::now() + Duration::from_secs(5));
    }
    let git = f::git_state(&actual.fixture);
    let refs = f::git(&actual.fixture, &["show-ref"]);
    let working_diff = f::git(&actual.fixture, &["diff", "--binary"]);
    let staged_diff = f::git(&actual.fixture, &["diff", "--cached", "--binary"]);
    let config = fs::read(actual.fixture.repo().join(".git/config")).unwrap();
    let (reached, paused) = mpsc::sync_channel(1);
    let (release, resume) = mpsc::sync_channel(1);
    let gate = matches!(
        fault,
        Fault::LateNegativeCancel
            | Fault::LatePositiveCancel
            | Fault::LateNegativeDeadline
            | Fault::LatePositiveDeadline
    )
    .then(|| {
        Arc::new(ReceiveGateV9 {
            reached,
            release: std::sync::Mutex::new(resume),
            used: std::sync::atomic::AtomicBool::new(false),
        })
    });
    thread::scope(|scope| {
        let publisher = scope.spawn(|| {
            let _gate = gate
                .as_ref()
                .map(|gate| install_receive_gate_v9(gate.clone()));
            confirm
                .execute(&actual.provider, &path, &client, fence)
                .unwrap()
        });
        let saved = next(&actual.receiver);
        assert!(
            matches!(&saved,Message::PublicationV2(message) if matches!(&message.effect,PublicationEffectV2::Save(body) if body.len()<16*1024&&body.contains("scoped-actions-workflow-remote-v9")))
        );
        dispatch(&actual.writer, saved, &cache);
        let Message::PublicationV2(mut message) = next(&actual.receiver) else {
            panic!("Actual accepted closed cleanup")
        };
        assert!(matches!(
            &message.effect,
            PublicationEffectV2::CommitWorkflowCleanupV9(_)
        ));
        let until = Instant::now() + Duration::from_secs(8);
        while !confirm.cleanup_waiting_v9() {
            assert!(
                Instant::now() < until,
                "Actual accepted waiter observation after SAME queue receive"
            );
            thread::yield_now();
        }
        if gate.is_some() {
            paused
                .recv_timeout(Duration::from_secs(8))
                .expect("Actual waiter passed original top current-check and is held BEFORE recv");
        }
        let PublicationEffectV2::CommitWorkflowCleanupV9(payload) = &message.effect else {
            unreachable!("Actual closed cleanup required above")
        };
        let prepared = payload.prepared.as_ref().unwrap();
        assert!(prepared.matches_request_v9(&confirm));
        assert_eq!(prepared.history_v9().reference().attempt_id, 1);
        let effective = prepared.effective_v9();
        let actual_binding_inputs = matches!(fault, Fault::ForeignAck).then(|| {
            (
                prepared.history_v9().clone(),
                prepared.test_original_heap_v9().clone(),
                prepared.effective_v9(),
            )
        });
        exact(&actual.fixture, &actual.steps);
        let original_rows = sql(&path);
        let body = cache
            .as_ref()
            .unwrap()
            .draft(intent.source().original().scope(), intent.key())
            .unwrap()
            .unwrap();
        let row = original_rows
            .iter()
            .find(|(name, _)| name == "drafts")
            .unwrap()
            .1
            .iter()
            .find(|row| matches!(row.get(2), Some(SqlValue::Text(key)) if key=="scoped-actions-v9/repository/10000/workflow/7/local-edit"))
            .unwrap()
            .clone();
        assert!(
            body.contains("actions_publication") && !body.contains("PRIVATE_ACTUAL_CLEANUP_EDIT")
        );
        if matches!(
            fault,
            Fault::RawReceipt | Fault::LateNegativeCancel | Fault::LateNegativeDeadline
        ) {
            let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap();
            assert_eq!(db.execute("UPDATE actions_write_attempts SET receipt_json=receipt_json||' ' WHERE attempt_id=1",[]).unwrap(),1);
        }
        let before = sql(&path);
        let (proxy, observed) = mpsc::sync_channel(1);
        let PublicationReplyV85::WorkflowCleanupV9(original) = std::mem::replace(
            &mut message.reply,
            PublicationReplyV85::WorkflowCleanupV9(proxy),
        ) else {
            panic!("Actual cleanup reply")
        };
        let mut foreign = None;
        if matches!(fault, Fault::ForeignCache | Fault::ForeignAck) {
            let foreign_path = path.with_file_name("genuine-foreign-cleanup.sqlite3");
            fs::copy(&path, &foreign_path).unwrap();
            let foreign_cache = Ok(Cache::open(&foreign_path).unwrap());
            if matches!(fault, Fault::ForeignAck) {
                // A second independently real writer grants only a genuine
                // ACK for its OWN database, even with the same small body.
                let writer = DraftWriter::new(foreign_path.clone());
                let client = writer.client();
                let fence = client
                    .capture_workflow_edit_publication_v9(
                        &foreign_path,
                        intent,
                        &token,
                        confirm.deadline(),
                    )
                    .unwrap();
                let ack = client
                    .save_publication_and_wait_v2(fence, body.clone(), &token)
                    .unwrap();
                ack.check_current(
                    &client,
                    &foreign_path,
                    intent.source().original().scope(),
                    intent.key(),
                    &token,
                )
                .unwrap();
                let foreign_before = sql(&foreign_path);
                let (history, heap, effective) = actual_binding_inputs.as_ref().unwrap();
                let remaining = heap.remaining_v9().unwrap();
                let binding = ack
                    .prepare_workflow_cleanup_binding_v9(
                        &client,
                        WorkflowCleanupInputV9 {
                            request: &confirm,
                            history,
                            claim: history.test_cleanup_claim_v9(),
                            encoded: body.clone(),
                            heap,
                            cancel: &token,
                            effective: *effective,
                        },
                    )
                    .unwrap();
                assert!(foreign_cache.as_ref().unwrap().prepare_workflow_cleanup_v9(
                    &confirm, history, binding, heap,
                ).is_err(), "REAL foreign-ACK binding must fail REAL prepare original issuer before Tx/raw copies");
                assert_eq!(
                    heap.remaining_v9().unwrap(),
                    remaining,
                    "Actual SAME phase ledger is restored; no reset/reconstructed budget"
                );
                assert_eq!(sql(&path), before);
                assert_eq!(sql(&foreign_path), foreign_before);
                // Actual opaque request/history and a genuine mechanical ACK
                // still cannot construct or replace the original SQL issuer.
                drop(ack);
                drop(client);
                drop(writer);
            }
            let foreign_before = sql(&foreign_path);
            dispatch(
                &actual.writer,
                Message::PublicationV2(message),
                &foreign_cache,
            );
            assert_eq!(sql(&foreign_path), foreign_before);
            foreign = Some((foreign_path, foreign_before));
        } else {
            dispatch(&actual.writer, Message::PublicationV2(message), &cache);
        }
        let result = observed.recv_timeout(Duration::from_secs(8)).unwrap();
        let removed = matches!(
            fault,
            Fault::None
                | Fault::LostReply
                | Fault::LatePositiveCancel
                | Fault::LatePositiveDeadline
        );
        assert_eq!(
            result.is_ok(),
            removed,
            "Actual real worker outcome {fault:?}"
        );
        if removed {
            assert_eq!(sql(&path), without_draft(before.clone(), &row));
            assert_eq!(
                cache
                    .as_ref()
                    .unwrap()
                    .draft(intent.source().original().scope(), intent.key())
                    .unwrap(),
                None
            );
        } else {
            assert!(matches!(&result, Err(WorkflowCleanupFailureV9::Retained)));
            assert_eq!(sql(&path), before);
            assert_eq!(
                cache
                    .as_ref()
                    .unwrap()
                    .draft(intent.source().original().scope(), intent.key())
                    .unwrap()
                    .as_deref(),
                Some(body.as_str())
            );
        }
        if matches!(fault, Fault::LateNegativeCancel | Fault::LatePositiveCancel) {
            token.cancel();
        }
        if matches!(
            fault,
            Fault::LateNegativeDeadline | Fault::LatePositiveDeadline
        ) {
            assert!(
                Instant::now() < effective,
                "Real worker replied while SAME I was current"
            );
            assert!(
                effective < confirm.deadline(),
                "Actual narrower I, not original request D"
            );
            while Instant::now() < effective {
                thread::sleep(Duration::from_millis(1));
            }
            assert!(
                !token.is_cancelled(),
                "Deadline path does not substitute cancellation"
            );
            assert!(
                Instant::now() < confirm.deadline(),
                "Original request D still live"
            );
        }
        if matches!(fault, Fault::LostReply) {
            drop(original);
            drop(result);
        } else {
            assert!(original.send(result).is_ok(), "Genuine worker reply delivered to SAME original waiter, including after token cancellation");
        }
        if gate.is_some() {
            release.send(()).unwrap();
        }
        let outcome = publisher.join().unwrap();
        assert!(
            !confirm.cleanup_waiting_v9(),
            "RAII clears accepted observation on every actual result"
        );
        let expected = match fault {
            Fault::None => None,
            Fault::RawReceipt | Fault::ForeignCache | Fault::ForeignAck => {
                Some(WorkflowCleanupFailureV9::Retained)
            }
            Fault::LateNegativeCancel
            | Fault::LateNegativeDeadline
            | Fault::LostReply
            | Fault::LatePositiveCancel
            | Fault::LatePositiveDeadline => Some(WorkflowCleanupFailureV9::Unknown),
        };
        assert_eq!(outcome.cleanup_failure_v9(), expected);
        let published = outcome.into_published_v9();
        assert_eq!(published.is_some(), matches!(fault, Fault::None));
        if let Some(published) = published {
            assert!(published.branch.is_some() && published.history.is_some());
            assert!(Arc::ptr_eq(
                published.branch.as_ref().unwrap().proof_v9(),
                published.history.as_ref().unwrap()
            ));
        }
        assert_eq!(f::git_state(&actual.fixture), git);
        assert_eq!(f::git(&actual.fixture, &["show-ref"]), refs);
        assert_eq!(f::git(&actual.fixture, &["diff", "--binary"]), working_diff);
        assert_eq!(
            f::git(&actual.fixture, &["diff", "--cached", "--binary"]),
            staged_diff
        );
        assert_eq!(
            fs::read(actual.fixture.repo().join(".github/workflows/ci.yml")).unwrap(),
            EDITED.as_bytes()
        );
        assert_eq!(
            fs::read(actual.fixture.repo().join(".git/config")).unwrap(),
            config
        );
        exact(&actual.fixture, &actual.steps);
        if let Some((foreign, before)) = foreign {
            assert_eq!(sql(&foreign), before);
        }
    });
    // No waiter, reply, controller or observation can enqueue a second write.
    assert!(matches!(
        actual.receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    let bare = actual.tls.bare_repository().to_str().unwrap();
    let observed = f::git(&actual.fixture, &["--git-dir", bare, "show-ref"]);
    let mut observed = observed.lines().map(str::to_owned).collect::<Vec<_>>();
    let mut expected = vec![
        format!("{} refs/heads/main", actual.original),
        format!("{} refs/heads/{BRANCH}", actual.committed),
    ];
    observed.sort();
    expected.sort();
    assert_eq!(
        observed, expected,
        "Actual create-only Push D happened exactly once; original main/C remains unchanged"
    );
    drop(client);
    drop(actual.writer);
    let report = actual.tls.finish().unwrap();
    assert_eq!(report.exchanges, report.requests.len());
    assert!(report.exchanges <= 16);
    assert_eq!(report.io_threads_joined, 1 + 2 * report.exchanges);
    assert!(report
        .processes
        .iter()
        .all(|p| p.application_pid > 0 && p.remaining.is_empty() && p.errors.is_empty()));
}

#[test]
fn task54_workflow_edit_v9_cleanup_actual_queue_positive_exact_ack_alone_releases_branch() {
    actual_case(Fault::None)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_queue_raw_refusal_retains_every_exact_row() {
    actual_case(Fault::RawReceipt)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_accepted_negative_delivered_after_cancel_is_unknown() {
    actual_case(Fault::LateNegativeCancel)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_committed_worker_reply_loss_cannot_release_branch() {
    actual_case(Fault::LostReply)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_positive_reply_after_cancel_never_releases_branch() {
    actual_case(Fault::LatePositiveCancel)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_negative_reply_after_original_effective_i_is_unknown() {
    actual_case(Fault::LateNegativeDeadline)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_positive_reply_after_original_effective_i_never_releases_branch(
) {
    actual_case(Fault::LatePositiveDeadline)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_request_and_history_refuse_mirrored_foreign_worker_cache()
{
    actual_case(Fault::ForeignCache)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_request_history_and_real_foreign_same_body_ack_do_not_replace_issuer(
) {
    actual_case(Fault::ForeignAck)
}

fn after_actual_claim_and_receipt(mut pending: SqlImage, d: &str) -> SqlImage {
    // Exact independently expected transition. No table or caller bytes are
    // excluded: one actual pending marker is claimed/confirmed; own gens1→3.
    let mut exact_tables = 0;
    for (name, rows) in &mut pending {
        if name == "actions_write_attempts" {
            exact_tables += 1;
            assert_eq!(rows.len(), 1);
            let row = &mut rows[0];
            assert_eq!(row.len(), 13);
            assert_eq!(row[0], SqlValue::Integer(1));
            assert_eq!(row[9], SqlValue::Text("pending".into()));
            assert_eq!(row[10], SqlValue::Integer(0));
            assert_eq!(row[11], SqlValue::Null);
            row[9] = SqlValue::Text("confirmed".into());
            row[10] = SqlValue::Integer(1);
            row[11] = SqlValue::Text(format!("{{\"kind\":\"workflow_branch_observed\",\"resource_id\":10000,\"observed_attempt\":null,\"observed_run_id\":null,\"workflow_remote_v9\":{{\"phase\":\"branch\",\"head_repository_id\":10000,\"head_repository_node\":\"R_10000\",\"head_ref\":\"workflow/edit\",\"committed_head\":\"{d}\"}}}}"));
        } else if name == "actions_write_generations" {
            exact_tables += 1;
            assert_eq!(
                rows.len(),
                4,
                "Same original/current name means FOUR distinct own aliases"
            );
            for row in rows {
                assert_eq!(row.len(), 4);
                assert_eq!(row[3], SqlValue::Integer(1));
                row[3] = SqlValue::Integer(3);
            }
        } else if name == "actions_write_resources" {
            exact_tables += 1;
            let mut observed = rows
                .iter()
                .map(|row| {
                    assert_eq!(row.len(), 4);
                    assert_eq!(
                        (&row[0], &row[1], &row[3]),
                        (
                            &SqlValue::Text(HOST.into()),
                            &SqlValue::Text("user-id:42".into()),
                            &SqlValue::Integer(1)
                        )
                    );
                    row[2].clone()
                })
                .collect::<Vec<_>>();
            let mut expected = [
                format!("{HOST}/{REPOSITORY}/workflow/7"),
                format!("{HOST}/repository-id/10000/actions/workflow/7"),
                format!("{HOST}/{REPOSITORY}/branch/{BRANCH}"),
                format!("{HOST}/repository-id/10000/git/branch/{BRANCH}"),
            ]
            .into_iter()
            .map(SqlValue::Text)
            .collect::<Vec<_>>();
            observed.sort_by_key(|row| format!("{row:?}"));
            expected.sort_by_key(|row| format!("{row:?}"));
            assert_eq!(observed, expected);
        }
    }
    assert_eq!(
        exact_tables, 3,
        "All actual journal schemas/rows must exist; absence is never default equality"
    );
    pending
}
fn admission_case(disconnected: bool) {
    let mut actual = actual();
    let path = f::database(&actual.fixture);
    let cache = Ok(Cache::open(&path).unwrap());
    let client = actual.writer.client();
    let work = w::request(&mut actual.app, FormAction::Submit);
    let Some(publication::PublicationWorkV3::WorkflowEdit(publication)) =
        work.publication_v3.as_deref()
    else {
        panic!("Actual workflow work")
    };
    let publication::workflow_edit_v9::WorkflowEditWorkV9::Form {
        confirm: Some(publication::workflow_edit_v9::WorkflowEditConfirmV9::Remote(confirm)),
        ..
    } = &**publication
    else {
        panic!("Actual native captured Confirm")
    };
    let confirm = confirm.clone();
    let token = confirm.cancellation().clone();
    let intent = confirm
        .preview()
        .selection()
        .committed()
        .staged()
        .saved()
        .opened()
        .intent();
    let mut fence = client
        .capture_workflow_edit_publication_v9(&path, intent, &token, confirm.deadline())
        .unwrap();
    if !disconnected {
        // Restrict the GENUINE fence BEFORE Core/post construction, so the
        // actual phase/pool/cursor ALL inherit this narrower original I.
        fence.deadline = fence.deadline.min(Instant::now() + Duration::from_secs(5));
    }
    let git = f::git_state(&actual.fixture);
    let refs = f::git(&actual.fixture, &["show-ref"]);
    let mut receiver = Some(actual.receiver);
    thread::scope(|scope| {
        let publisher = scope.spawn(|| {
            confirm
                .execute(&actual.provider, &path, &client, fence)
                .unwrap()
        });
        let Message::PublicationV2(mut saved) = next(receiver.as_ref().unwrap()) else {
            panic!("Actual reference Save")
        };
        assert!(matches!(&saved.effect, PublicationEffectV2::Save(body) if body.len()<16*1024));
        let (proxy, observed) = mpsc::sync_channel(1);
        let PublicationReplyV85::Draft(original) =
            std::mem::replace(&mut saved.reply, PublicationReplyV85::Draft(proxy))
        else {
            panic!("Actual same Save ACK")
        };
        dispatch(&actual.writer, Message::PublicationV2(saved), &cache);
        let ack = observed
            .recv_timeout(Duration::from_secs(8))
            .unwrap()
            .unwrap();
        ack.check_current(
            &client,
            &path,
            intent.source().original().scope(),
            intent.key(),
            &token,
        )
        .unwrap();
        let body = cache
            .as_ref()
            .unwrap()
            .draft(intent.source().original().scope(), intent.key())
            .unwrap()
            .unwrap();
        let pending = sql(&path);
        assert!(!confirm.cleanup_waiting_v9());
        if disconnected {
            drop(receiver.take()); // ACTUAL original channel; no fake error.
        } else {
            // Same fixed-eight physical channel and inert never-run sentinels
            // as existing V85 admission tests. No body/claim/source minted.
            for _ in 0..8 {
                assert!(actual.writer.sender.try_send(Message::Shutdown).is_ok());
            }
        }
        assert!(original.send(Ok(ack)).is_ok());
        let outcome = publisher.join().unwrap();
        assert_eq!(
            outcome.cleanup_failure_v9(),
            Some(WorkflowCleanupFailureV9::Retained),
            "Pre-enqueue refusal differs from accepted UNKNOWN"
        );
        assert!(outcome.into_published_v9().is_none());
        assert!(
            !confirm.cleanup_waiting_v9(),
            "Failed TrySend never exposes acceptance"
        );
        assert_eq!(
            sql(&path),
            after_actual_claim_and_receipt(pending, &actual.committed)
        );
        assert_eq!(
            cache
                .as_ref()
                .unwrap()
                .draft(intent.source().original().scope(), intent.key())
                .unwrap()
                .as_deref(),
            Some(body.as_str())
        );
        assert_eq!(f::git_state(&actual.fixture), git);
        assert_eq!(f::git(&actual.fixture, &["show-ref"]), refs);
        exact(&actual.fixture, &actual.steps);
        assert_eq!(actual.writer.producer.lock().unwrap().legacy_active, 0);
        if let Some(receiver) = &receiver {
            for _ in 0..8 {
                assert!(matches!(receiver.try_recv(), Ok(Message::Shutdown)));
            }
            assert!(matches!(
                receiver.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
        }
    });
    let bare = actual.tls.bare_repository().to_str().unwrap();
    assert_eq!(
        f::git(
            &actual.fixture,
            &["--git-dir", bare, "rev-parse", "refs/heads/main"]
        )
        .trim(),
        actual.original
    );
    assert_eq!(
        f::git(
            &actual.fixture,
            &["--git-dir", bare, "rev-parse", "refs/heads/workflow/edit"]
        )
        .trim(),
        actual.committed
    );
    drop(client);
    drop(actual.writer);
    let report = actual.tls.finish().unwrap();
    assert_eq!(report.exchanges, report.requests.len());
    assert_eq!(report.io_threads_joined, 1 + 2 * report.exchanges);
    assert!(
        report.exchanges <= 16
            && report
                .processes
                .iter()
                .all(|p| p.application_pid > 0 && p.remaining.is_empty() && p.errors.is_empty())
    );
}
#[test]
fn task54_workflow_edit_v9_cleanup_genuine_full_pre_enqueue_channel_keeps_protected_draft_without_acceptance(
) {
    admission_case(false)
}
#[test]
fn task54_workflow_edit_v9_cleanup_genuine_disconnected_pre_enqueue_channel_keeps_protected_draft_without_acceptance(
) {
    admission_case(true)
}
