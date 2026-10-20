//! TASK-42/43: history previews list the exact affected commits and reset losses;
//! rebase, cherry-pick and revert stop on conflicts and continue, skip or abort.
use super::execute;
use crate::git_support::{git, Fixture};
use ghtui::git::{Operation, RawPath, ResetMode, Sequencer};

fn affected(fixture: &Fixture, operation: Operation) -> String {
    let status = fixture.repository.status().unwrap();
    fixture
        .repository
        .affected_preview_v1(&operation, &status)
        .unwrap()
        .join("\n")
}

fn rev(fixture: &Fixture, revision: &str) -> String {
    String::from_utf8(git(&fixture.root, ["rev-parse", revision]).stdout)
        .unwrap()
        .trim()
        .to_owned()
}

fn read(fixture: &Fixture, path: &str) -> Vec<u8> {
    std::fs::read(fixture.root.join(path)).unwrap()
}

fn stage(fixture: &Fixture, path: &str, content: &[u8]) {
    fixture.write(path, content);
    execute(
        fixture,
        Operation::Stage(vec![RawPath::new(path.as_bytes().to_vec())]),
    );
}

fn stop_with_conflict(fixture: &Fixture, operation: Operation, expected: Sequencer) {
    let plan = fixture.repository.plan(operation).unwrap();
    assert!(fixture.repository.execute(&plan, true).is_err());
    fixture.repository.reconcile_write().unwrap();
    let status = fixture.repository.status().unwrap();
    assert_eq!(status.operation, Some(expected));
    assert_eq!(status.conflicts(), 1);
}

fn assert_finished(fixture: &Fixture) {
    assert_eq!(fixture.repository.sequencer(), None);
    assert!(fixture.repository.status().unwrap().clean());
}

/// Topic changes readme.txt then adds extra.txt; main changes readme.txt differently.
fn conflicting() -> (Fixture, String, String, String) {
    let fixture = Fixture::linear();
    git(&fixture.root, ["switch", "-c", "topic"]);
    fixture.write("readme.txt", b"topic conflict\n");
    let topic_conflict = fixture.commit("Topic conflict");
    fixture.write("extra.txt", b"extra\n");
    let topic_extra = fixture.commit("Topic extra");
    git(&fixture.root, ["switch", "main"]);
    fixture.write("readme.txt", b"main conflict\n");
    let main = fixture.commit("Main conflict");
    (fixture, topic_conflict, topic_extra, main)
}

#[test]
fn git_sequencers_previews_list_affected_commits_and_exact_reset_losses() {
    let (fixture, topic_conflict, topic_extra, main) = conflicting();
    let merge = affected(&fixture, Operation::Merge("topic".into()));
    assert!(
        merge.contains("Commits this merge brings in (2):"),
        "{merge}"
    );
    assert!(merge.contains(&format!("{topic_conflict} Topic conflict")));
    assert!(merge.contains(&format!("{topic_extra} Topic extra")));
    assert!(!merge.contains(&main));
    let rebase = affected(&fixture, Operation::Rebase("topic".into()));
    assert!(
        rebase.starts_with(&format!(
            "Commits replayed onto {topic_extra} (1):\n  {main} Main conflict"
        )),
        "{rebase}"
    );
    assert!(!rebase.contains(&topic_conflict));
    let pick = affected(&fixture, Operation::CherryPick(topic_extra.clone()));
    assert!(
        pick.contains(&format!(
            "Commit applied on top of HEAD (1):\n  {topic_extra} Topic extra"
        )),
        "{pick}"
    );
    let revert = affected(&fixture, Operation::Revert("HEAD".into()));
    assert!(
        revert.contains(&format!("{main} Main conflict")),
        "{revert}"
    );

    fixture.write("readme.txt", b"dirty work\n");
    fixture.write("staged.txt", b"staged\n");
    git(&fixture.root, ["add", "staged.txt"]);
    fixture.write("untracked.txt", b"kept\n");
    let reset = |mode| {
        affected(
            &fixture,
            Operation::Reset {
                revision: "main~1".into(),
                mode,
            },
        )
    };
    let hard = reset(ResetMode::Hard);
    assert!(
        hard.contains(&format!("(1):\n  {main} Main conflict")),
        "{hard}"
    );
    assert!(
        hard.contains("Uncommitted tracked changes discarded (2):"),
        "{hard}"
    );
    assert!(hard.contains("  readme.txt") && hard.contains("  staged.txt"));
    assert!(!hard.contains("untracked.txt") && hard.contains("Untracked files are kept."));
    let mixed = reset(ResetMode::Mixed);
    assert!(
        mixed.contains("Staged changes unstaged; working files kept (1):\n  staged.txt"),
        "{mixed}"
    );
    assert!(!mixed.contains("readme.txt"));
    assert!(reset(ResetMode::Soft).contains("Index and working files are kept (0):"));
    // Previews are read-only: nothing moved and the dirty work is intact.
    assert_eq!(fixture.head(), main);
    assert_eq!(read(&fixture, "readme.txt"), b"dirty work\n");
    assert_eq!(read(&fixture, "untracked.txt"), b"kept\n");
}

#[test]
fn git_sequencers_rebase_conflict_skip_abort_and_continue() {
    let (fixture, topic_conflict, _, main) = conflicting();
    git(&fixture.root, ["switch", "topic"]);
    let original = fixture.head();
    stop_with_conflict(
        &fixture,
        Operation::Rebase("main".into()),
        Sequencer::Rebase,
    );
    let skip = affected(&fixture, Operation::Skip);
    assert!(
        skip.contains("skipped") && skip.contains(&format!("{topic_conflict} Topic conflict")),
        "{skip}"
    );
    execute(&fixture, Operation::Skip);
    assert_finished(&fixture);
    assert_eq!(rev(&fixture, "HEAD^"), main);
    assert_eq!(read(&fixture, "readme.txt"), b"main conflict\n");
    assert_eq!(read(&fixture, "extra.txt"), b"extra\n");

    git(&fixture.root, ["reset", "--hard", &original]);
    fixture.repository.reconcile_write().unwrap();
    stop_with_conflict(
        &fixture,
        Operation::Rebase("main".into()),
        Sequencer::Rebase,
    );
    assert!(affected(&fixture, Operation::Abort).contains(&topic_conflict));
    execute(&fixture, Operation::Abort);
    assert_eq!(fixture.head(), original);
    assert_finished(&fixture);

    stop_with_conflict(
        &fixture,
        Operation::Rebase("main".into()),
        Sequencer::Rebase,
    );
    stage(&fixture, "readme.txt", b"resolved\n");
    assert!(affected(&fixture, Operation::Continue).contains(&topic_conflict));
    execute(&fixture, Operation::Continue);
    assert_finished(&fixture);
    assert_eq!(rev(&fixture, "HEAD~2"), main);
    assert_eq!(
        git(&fixture.root, ["show", "HEAD~1:readme.txt"]).stdout,
        b"resolved\n"
    );
}

#[test]
fn git_sequencers_cherry_pick_and_revert_continue_skip_and_abort() {
    let (fixture, topic_conflict, _, main) = conflicting();
    let pick = || Operation::CherryPick(topic_conflict.clone());
    stop_with_conflict(&fixture, pick(), Sequencer::CherryPick);
    assert!(
        affected(&fixture, Operation::Abort).contains(&format!("{topic_conflict} Topic conflict"))
    );
    execute(&fixture, Operation::Abort);
    assert_eq!(fixture.head(), main);
    assert_finished(&fixture);

    stop_with_conflict(&fixture, pick(), Sequencer::CherryPick);
    stage(&fixture, "readme.txt", b"picked\n");
    execute(&fixture, Operation::Continue);
    assert_finished(&fixture);
    assert_eq!(rev(&fixture, "HEAD^"), main);
    assert_eq!(
        git(&fixture.root, ["show", "HEAD:readme.txt"]).stdout,
        b"picked\n"
    );
    let picked = fixture.head();

    // Reverting "Update readme" conflicts with the later readme changes.
    let update = rev(&fixture, "main~2");
    let revert = || Operation::Revert(update.clone());
    stop_with_conflict(&fixture, revert(), Sequencer::Revert);
    assert!(affected(&fixture, Operation::Skip).contains(&format!("{update} Update readme")));
    execute(&fixture, Operation::Skip);
    assert_eq!(fixture.head(), picked);
    assert_finished(&fixture);

    stop_with_conflict(&fixture, revert(), Sequencer::Revert);
    execute(&fixture, Operation::Abort);
    assert_eq!(fixture.head(), picked);
    assert_finished(&fixture);

    stop_with_conflict(&fixture, revert(), Sequencer::Revert);
    stage(&fixture, "readme.txt", b"reverted\n");
    execute(&fixture, Operation::Continue);
    assert_finished(&fixture);
    assert_eq!(rev(&fixture, "HEAD^"), picked);
}
