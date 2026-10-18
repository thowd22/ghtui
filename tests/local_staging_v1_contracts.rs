//! Direct + / - staging: the shortcut picks the exact command, opens with a
//! ready read-only preview, writes only after execute(confirmed) and keeps
//! the user in the diff they were working through.
mod support;
use ghtui::{
    diff::LineKind,
    forms::{Form, FormPurpose},
    git::RawPath,
    local_actions::{
        completion_target, preview_form, refreshed_mode, LocalCommand, LocalContext, QuickLocal,
    },
    providers::runner::CancellationToken,
    repository_view::DiffAnchor,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use std::fs;
use support::Fixture;

fn workspace(fixture: &Fixture, mode: RepositoryMode) -> Workspace {
    Workspace::load(&fixture.repo(), mode, "HEAD", None)
        .unwrap()
        .unwrap()
}

fn numbered(lines: &[&str]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn original() -> Vec<String> {
    (1..=20).map(|index| format!("l{index}")).collect()
}

/// Every changed line of one hunk, as a hunk selection produces them.
fn hunk_context(view: &Workspace, hunk: usize) -> LocalContext {
    let anchors: Vec<DiffAnchor> = view
        .rows
        .iter()
        .filter_map(|row| match &row.action {
            Some(RowAction::DiffLine(anchor)) if anchor.hunk_index == hunk => Some(anchor.clone()),
            _ => None,
        })
        .collect();
    let diff = &view.comparison.as_ref().unwrap().diff;
    let mut context = LocalContext::from_workspace(view, None, anchors.first().cloned());
    context.selected_lines = anchors
        .iter()
        .filter(|anchor| {
            diff.files[anchor.file_index].hunks[anchor.hunk_index].lines[anchor.line_index].kind
                != LineKind::Context
        })
        .map(|anchor| (anchor.hunk_index, anchor.line_index))
        .collect();
    context
}

#[test]
fn v1_local_staging_quick_command_maps_rows_and_selections_exactly() {
    let file = RowAction::WorkingFile(RawPath::new(b"a.txt".to_vec()));
    assert_eq!(
        QuickLocal::Stage.command(Some(&file), false),
        Ok(LocalCommand::StageFile)
    );
    assert_eq!(
        QuickLocal::Unstage.command(Some(&file), false),
        Ok(LocalCommand::UnstageFile)
    );
    // An explicit line/hunk selection wins over the row under the cursor.
    assert_eq!(
        QuickLocal::Stage.command(Some(&file), true),
        Ok(LocalCommand::StageSelection)
    );
    assert_eq!(
        QuickLocal::Unstage.command(None, true),
        Ok(LocalCommand::UnstageSelection)
    );
    assert_eq!(
        QuickLocal::Commit.command(None, false),
        Ok(LocalCommand::Commit)
    );
    // Nothing is guessed for headers, commits or empty rows.
    assert!(QuickLocal::Stage.command(None, false).is_err());
    assert!(QuickLocal::Unstage
        .command(Some(&RowAction::Commit("abc".into())), false)
        .is_err());
    assert!(QuickLocal::previews_immediately(
        LocalCommand::StageSelection
    ));
    assert!(!QuickLocal::previews_immediately(LocalCommand::Commit));
}

#[test]
fn v1_local_staging_quick_preview_writes_only_after_confirmation_and_stays_in_working_diff() {
    let fixture = Fixture::new();
    fixture.init_git();
    let saved = original();
    let saved_refs: Vec<&str> = saved.iter().map(String::as_str).collect();
    fixture.commit_file("file.txt", numbered(&saved_refs).as_bytes(), "saved");
    let mut working = saved.clone();
    working[1] = "first".into();
    working[18] = "second".into();
    let working_refs: Vec<&str> = working.iter().map(String::as_str).collect();
    let working_bytes = numbered(&working_refs);
    fs::write(fixture.repo().join("file.txt"), &working_bytes).unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);

    let view = workspace(&fixture, RepositoryMode::Working);
    let context = hunk_context(&view, 0);
    assert_eq!(context.selected_lines.len(), 2);
    assert_eq!(
        refreshed_mode(LocalCommand::StageSelection, &context),
        RepositoryMode::Working
    );
    assert_eq!(
        refreshed_mode(LocalCommand::Commit, &context),
        RepositoryMode::Overview
    );
    assert_eq!(
        completion_target(LocalCommand::StageSelection, &context),
        " • 2 lines in file.txt"
    );
    let mut form = Form::local(LocalCommand::StageSelection, context).unwrap();
    preview_form(&mut form, CancellationToken::default()).unwrap();
    assert!(form.preview);
    let FormPurpose::Local(intent) = &form.purpose else {
        panic!("local form")
    };
    let plan = intent.plan.as_ref().expect("prepared preview");
    assert!(plan.preview.iter().any(|line| line == "+first"));
    assert!(!plan.preview.iter().any(|line| line == "+second"));
    // Preview is read-only: the index still has the saved bytes.
    assert_eq!(fixture.git(&["show", ":file.txt"]), numbered(&saved_refs));
    assert!(plan.execute(false, CancellationToken::default()).is_err());
    assert_eq!(fixture.git(&["show", ":file.txt"]), numbered(&saved_refs));

    let outcome = plan.execute(true, CancellationToken::default()).unwrap();
    let mut staged = saved.clone();
    staged[1] = "first".into();
    let staged_refs: Vec<&str> = staged.iter().map(String::as_str).collect();
    assert_eq!(fixture.git(&["show", ":file.txt"]), numbered(&staged_refs));
    assert_eq!(outcome.workspace.mode, RepositoryMode::Working);
    assert!(outcome
        .workspace
        .rows
        .iter()
        .any(|row| row.label.ends_with("+second")));
    assert!(!outcome
        .workspace
        .rows
        .iter()
        .any(|row| row.label.ends_with("+first")));
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        working_bytes.as_bytes()
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);

    // Unstaging the same hunk from Staged diff returns there.
    let staged_view = workspace(&fixture, RepositoryMode::Staged);
    let context = hunk_context(&staged_view, 0);
    let mut form = Form::local(LocalCommand::UnstageSelection, context).unwrap();
    preview_form(&mut form, CancellationToken::default()).unwrap();
    let FormPurpose::Local(intent) = &form.purpose else {
        panic!("local form")
    };
    let outcome = intent
        .plan
        .as_ref()
        .unwrap()
        .execute(true, CancellationToken::default())
        .unwrap();
    assert_eq!(outcome.workspace.mode, RepositoryMode::Staged);
    assert_eq!(fixture.git(&["show", ":file.txt"]), numbered(&saved_refs));
}

#[test]
fn v1_local_staging_new_file_preview_has_no_phantom_trailing_line() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("saved.txt", b"saved\n", "saved");
    fs::write(fixture.repo().join("one.txt"), b"only line\n").unwrap();
    fs::write(fixture.repo().join("open.txt"), b"no newline").unwrap();
    fs::write(fixture.repo().join("empty.txt"), b"").unwrap();
    let view = workspace(&fixture, RepositoryMode::Overview);
    for (name, expected, absent) in [
        ("one.txt", "    1 only line", "    2"),
        ("open.txt", "\\ No newline at end of file", "    2"),
        ("empty.txt", "Empty new file: 0 bytes", "    1"),
    ] {
        let context =
            LocalContext::from_workspace(&view, Some(RawPath::new(name.as_bytes().to_vec())), None);
        assert_eq!(
            completion_target(LocalCommand::StageFile, &context),
            format!(" • {name}")
        );
        let mut form = Form::local(LocalCommand::StageFile, context).unwrap();
        preview_form(&mut form, CancellationToken::default()).unwrap();
        let FormPurpose::Local(intent) = &form.purpose else {
            panic!("local form")
        };
        let preview = &intent.plan.as_ref().unwrap().preview;
        assert!(preview.iter().any(|line| line == expected), "{preview:?}");
        assert!(
            !preview.iter().any(|line| line.starts_with(absent)),
            "{preview:?}"
        );
        assert_eq!(fixture.git(&["ls-files", "--", name]), "");
    }
}

#[test]
fn v1_local_staging_commit_is_disabled_until_something_is_staged() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("a.txt", b"old\n", "saved");
    fs::write(fixture.repo().join("a.txt"), b"new\n").unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    assert!(LocalCommand::Commit
        .disabled_reason(&context)
        .is_some_and(|reason| reason.starts_with("Nothing is staged yet")));
    // Amend can still reword the latest commit with nothing staged.
    assert_eq!(LocalCommand::Amend.disabled_reason(&context), None);
    fixture.git(&["add", "a.txt"]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    assert_eq!(LocalCommand::Commit.disabled_reason(&context), None);
}
