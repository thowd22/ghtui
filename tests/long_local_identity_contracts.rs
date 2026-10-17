mod support;
use ghtui::{
    app::{Action, App, Overlay, View},
    forms::{Form, FormAction},
    git::RawPath,
    local_actions::{LocalCommand, LocalContext, LocalInventory},
    local_picker::{LocalChoice, LocalPicker, LocalValue, PickerAction, PickerOutcome},
    providers::runner::CancellationToken,
    theme::Theme,
    workspace::{RepositoryMode, Workspace},
};
use ratatui::{backend::TestBackend, Terminal};
use std::fs;
use support::Fixture;

fn paint(
    picker: &mut LocalPicker,
    width: u16,
    height: u16,
) -> (String, Vec<ghtui::local_picker::PickerHit>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = vec![];
    terminal
        .draw(|frame| hits = picker.draw(frame, frame.area(), Theme::from_settings(0, true)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let text = buffer
        .content
        .chunks(usize::from(width))
        .map(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    (text, hits)
}
fn choice(raw: &[u8]) -> LocalChoice {
    let path = RawPath::new(raw.to_vec());
    LocalChoice {
        field: "path".into(),
        label: path.display(),
        detail: format!("{} • observed linked branch", "a".repeat(64)),
        editor_text: path.display(),
        value: LocalValue::Worktree {
            path,
            head: Some("a".repeat(64)),
            branch: Some("refs/heads/linked".into()),
        },
        disabled_reason: None,
    }
}

fn body_text(text: &str, width: u16, height: u16) -> String {
    text.lines()
        .skip(1)
        .take(usize::from(height.saturating_sub(4)))
        .map(|line| {
            line.chars()
                .skip(2)
                .take(usize::from(width.saturating_sub(4)))
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<String>()
}

#[test]
fn long_local_identity_details_exact_raw_choice_full_ids_and_both_action_hits() {
    let raw = format!("/{}/linked-", "long_original_root/".repeat(15));
    let mut bytes = raw.as_bytes().to_vec();
    bytes.push(0xff);
    let original = choice(&bytes);
    for (width, height) in [(80, 24), (120, 40)] {
        let mut picker = LocalPicker::from_choices(
            "Choose path • local inventory".into(),
            "path",
            vec![original.clone()],
            "Observed inventory only",
        );
        let (_, hits) = paint(&mut picker, width, height);
        let details = hits
            .iter()
            .find(|hit| hit.action == PickerAction::Details)
            .unwrap();
        assert!(details.rect.width >= 12 && details.rect.right() <= width);
        assert_eq!(
            LocalPicker::key(crossterm::event::KeyEvent::from(
                crossterm::event::KeyCode::F(2)
            )),
            Some(PickerAction::Details)
        );
        assert_eq!(picker.dispatch(details.action.clone()), PickerOutcome::None);
        let (text, hits) = paint(&mut picker, width, height);
        assert!(text.contains("Observed inventory target") && text.contains("NOT reverified"));
        assert!(text.contains("linked-\\xff"));
        assert!(body_text(&text, width, height).contains(&"a".repeat(64)));
        for action in [
            PickerAction::Page(-1),
            PickerAction::Page(1),
            PickerAction::Close,
            PickerAction::DetailEdge(false),
            PickerAction::DetailEdge(true),
        ] {
            assert_eq!(hits.iter().filter(|hit| hit.action == action).count(), 1);
        }
        assert_eq!(
            picker.dispatch(PickerAction::Choose),
            PickerOutcome::None,
            "No selection authority from a details overlay"
        );
        assert_eq!(
            picker.dispatch(PickerAction::Select(0)),
            PickerOutcome::None
        );
        assert_eq!(picker.options.as_slice(), std::slice::from_ref(&original));
        assert_eq!(picker.dispatch(PickerAction::Close), PickerOutcome::None);
        assert!(!paint(&mut picker, width, height)
            .0
            .contains("Observed inventory target"));
        assert_eq!(
            picker.dispatch(PickerAction::Choose),
            PickerOutcome::Chosen(Box::new(original.clone()))
        );
    }
}

#[test]
fn long_local_identity_details_wrong_slot_query_context_generation_and_clone_retire_before_draw() {
    for kind in 0..7 {
        let original = choice(b"/observed-original/linked-\xff");
        let other = choice(b"/changed-original/linked-unicode");
        let mut picker = LocalPicker::from_choices(
            "Choose path".into(),
            "path",
            vec![original, other],
            "observed",
        );
        picker.dispatch(PickerAction::Details);
        assert!(paint(&mut picker, 80, 24)
            .0
            .contains("Observed inventory target"));
        match kind {
            0 => picker.selected = 1,
            1 => picker.query.insert("changed query").unwrap(),
            2 => picker.title = "Different originating picker".into(),
            3 => picker.field = "other field".into(),
            4 => picker.options[0].value = LocalValue::Revision { id: "b".repeat(40) },
            5 => picker.visible[0] = usize::MAX,
            _ => picker = picker.clone(),
        }
        let (text, _) = paint(&mut picker, 80, 24);
        assert!(!text.contains("Observed inventory target"));
        assert!(!text.contains("Observed worktree directory"));
    }
    let mut picker = LocalPicker::from_choices(
        "Choose path".into(),
        "path",
        vec![choice(b"/root/one")],
        "observed",
    );
    picker.dispatch(PickerAction::Details);
    picker.dispatch(PickerAction::Close);
    picker.dispatch(PickerAction::Insert("no match".into()));
    assert!(picker.visible.is_empty());
    picker.dispatch(PickerAction::Details);
    assert!(!paint(&mut picker, 80, 24)
        .0
        .contains("Observed inventory target"));
}

#[test]
fn long_local_identity_details_malformed_oversized_disabled_and_terminal_controls_remain_read_only()
{
    let mut malicious = choice(b"/root/linked-\x1b[2J\n\t\\xff");
    malicious.disabled_reason = Some("Locked observed worktree; do not remove".into());
    let mut picker = LocalPicker::from_choices(
        "Choose path".into(),
        "path",
        vec![malicious.clone()],
        "observed",
    );
    picker.dispatch(PickerAction::Details);
    let (text, _) = paint(&mut picker, 80, 24);
    assert!(text.contains("\\u{1b}[2J\\n\\t\\\\xff"));
    assert!(text.contains("Locked observed worktree"));
    assert!(!text.contains('\x1b'));
    picker.dispatch(PickerAction::Close);
    assert_eq!(picker.dispatch(PickerAction::Choose), PickerOutcome::None);
    malicious.editor_text = "x".repeat(1024 * 1024);
    picker = LocalPicker::from_choices("Choose path".into(), "path", vec![malicious], "observed");
    picker.dispatch(PickerAction::Details);
    assert!(picker.message.contains("budget"));
    assert!(!paint(&mut picker, 80, 24)
        .0
        .contains("Observed inventory target"));
    let mut empty = LocalPicker::from_choices("Choose path".into(), "path", vec![], "observed");
    empty.dispatch(PickerAction::Details);
    assert!(empty.message.contains("No selected"));
}

#[test]
fn long_local_identity_real_git_inventory_details_preserve_exact_objects_and_checkout() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("saved", b"genuine exact object\n", "original source");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let inventory = LocalInventory::load(&fixture.repo(), CancellationToken::default()).unwrap();
    let mut picker = LocalPicker::new(LocalCommand::RemoveWorktree, "path", &inventory);
    assert_eq!(picker.options.len(), 1);
    assert!(picker.options[0].disabled_reason.is_some());
    let original = picker.options[0].clone();
    picker.dispatch(PickerAction::Details);
    let (text, _) = paint(&mut picker, 120, 40);
    assert!(text.contains("current/main workspace cannot be removed"));
    assert!(body_text(&text, 120, 40).contains(head.trim()));
    assert_eq!(picker.dispatch(PickerAction::Choose), PickerOutcome::None);
    picker.dispatch(PickerAction::Close);
    assert_eq!(picker.options[0], original);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("saved")).unwrap(),
        b"genuine exact object\n"
    );
}

#[test]
fn long_local_identity_local_full_input_view_is_unverified_no_selection_and_retires_on_edit() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("saved", b"original\n", "source");
    let workspace = Workspace::load(&fixture.repo(), RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    let context = LocalContext::from_workspace(&workspace, None, None);
    let mut form = Form::local(LocalCommand::RemoveWorktree, context).unwrap();
    let text = format!("/{}/linked-\\xff", "very_long_original_root/".repeat(15));
    form.fields[0].editor = ghtui::editor::TextBuffer::new(&text).unwrap();
    form.dispatch(&FormAction::FullInput(0));
    assert!(form.picker.as_ref().unwrap().is_current_input());
    assert!(form.picker.as_ref().unwrap().options.is_empty());
    let mut app = App::new();
    app.view = View::Repository;
    app.overlay = Some(Overlay::Form);
    app.form = Some(form);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| ghtui::ui::draw(frame, &mut app))
        .unwrap();
    assert!(!app.form.as_ref().unwrap().submit_requested);
    assert!(!app.form.as_ref().unwrap().preview_requested);
    app.form.as_mut().unwrap().fields[0]
        .editor
        .insert("changed")
        .unwrap();
    terminal
        .draw(|frame| ghtui::ui::draw(frame, &mut app))
        .unwrap();
    assert!(app.form.as_ref().unwrap().picker.is_none());
    assert!(app.form.as_ref().unwrap().error.contains("retired"));
    assert!(matches!(app.overlay, Some(Overlay::Form)));
    assert!(app
        .hits
        .iter()
        .any(|hit| hit.action == Action::Form(FormAction::FullInput(0))));
    assert!(!fixture.repo().join(".git/index.lock").exists());
}

#[test]
fn long_local_identity_input_purpose_directory_preview_secret_and_clone_retire_before_body() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("saved", b"source\n", "source");
    let workspace = Workspace::load(&fixture.repo(), RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    for kind in 0..6 {
        let mut form = Form::local(
            LocalCommand::RemoveWorktree,
            LocalContext::from_workspace(&workspace, None, None),
        )
        .unwrap();
        form.fields[0].editor =
            ghtui::editor::TextBuffer::new("/original-observed-private-intent").unwrap();
        form.dispatch(&FormAction::FullInput(0));
        let mut app = App::new();
        app.view = View::Repository;
        app.overlay = Some(Overlay::Form);
        app.form = Some(form);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| ghtui::ui::draw(frame, &mut app))
            .unwrap();
        let before: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(before.contains("Full current worktree input"));
        let form = app.form.as_mut().unwrap();
        match kind {
            0 => {
                if let ghtui::forms::FormPurpose::Local(intent) = &mut form.purpose {
                    intent.command = LocalCommand::AddWorktree;
                }
            }
            1 => {
                if let ghtui::forms::FormPurpose::Local(intent) = &mut form.purpose {
                    intent.context.directory = fixture.root.join("different-origin");
                }
            }
            2 => form.preview = true,
            3 => form.fields[0].secret = true,
            4 => form.submitting = true,
            _ => *form = form.clone(),
        }
        terminal
            .draw(|frame| ghtui::ui::draw(frame, &mut app))
            .unwrap();
        let after: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            !after.contains("Full current worktree input"),
            "old body suppressed for {kind}"
        );
        assert!(!after.contains("Entered intent text, NOT a verified"));
        assert!(!app.form.as_ref().unwrap().submit_requested);
        assert!(!app.form.as_ref().unwrap().preview_requested);
    }
}

#[test]
fn long_local_identity_details_scroll_resize_page_edges_and_capture_escape_budgets() {
    let original = choice(
        format!(
            "/{}/exact-ending",
            "bounded-original-directory/".repeat(300)
        )
        .as_bytes(),
    );
    let mut picker = LocalPicker::from_choices(
        "Choose path".into(),
        "path",
        vec![original.clone()],
        "observed",
    );
    picker.dispatch(PickerAction::Details);
    let (start, hits) = paint(&mut picker, 80, 24);
    assert!(start.contains("Observed inventory target"));
    assert!(hits
        .iter()
        .all(|hit| hit.rect.right() <= 80 && hit.rect.bottom() <= 24));
    picker.dispatch(PickerAction::DetailEdge(true));
    let end = paint(&mut picker, 80, 24).0;
    assert!(body_text(&end, 80, 24).contains("exact-ending"));
    assert!(!end.contains("Observed inventory target"));
    picker.dispatch(PickerAction::Page(-1));
    let previous = paint(&mut picker, 80, 24).0;
    assert_ne!(end, previous);
    picker.dispatch(PickerAction::Move(i32::MAX));
    let resized = paint(&mut picker, 120, 40).0;
    assert!(body_text(&resized, 120, 40).contains("exact-ending"));
    picker.dispatch(PickerAction::DetailEdge(false));
    assert!(paint(&mut picker, 120, 40)
        .0
        .contains("Observed inventory target"));
    let (_, narrow_hits) = paint(&mut picker, 40, 24);
    assert_eq!(narrow_hits.len(), 5);
    assert!(narrow_hits
        .iter()
        .all(|hit| hit.rect.right() <= 40 && hit.rect.width >= 10 && hit.rect.bottom() <= 24));
    picker.dispatch(PickerAction::Close);
    assert_eq!(picker.options, [original]);
    let mut escaped = choice(&vec![0xff; 9000]);
    // A compact label is not a waiver for the raw target's expansion budget.
    escaped.label = "bounded scalar label".into();
    escaped.editor_text = "bounded entered text".into();
    picker = LocalPicker::from_choices("Choose path".into(), "path", vec![escaped], "observed");
    picker.dispatch(PickerAction::Details);
    assert!(picker.message.contains("Escaped full details exceed"));
    assert!(!paint(&mut picker, 80, 24)
        .0
        .contains("Observed inventory target"));
    assert!(
        LocalPicker::current_input(&fixture_path(), "path", &"x".repeat(16 * 1024 + 1))
            .unwrap_err()
            .contains("budget")
    );
}

fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from("/observed-original-context")
}
