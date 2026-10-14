//! Prepared-source contracts; real-binary journeys separately exercise the UI.
use super::*;
use crate::{
    git::{Comparison, GitRepository, RawPath},
    providers::runner::CancellationToken,
    test_support::Fixture,
};
use std::{fs, path::Path, process::Command};

fn git(directory: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args([
            "-c",
            "user.name=Distinct Integrator",
            "-c",
            "user.email=integrator@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn sha(directory: &Path, revision: &str) -> String {
    String::from_utf8(git(directory, &["rev-parse", revision]))
        .unwrap()
        .trim()
        .into()
}
struct Real {
    fixture: Fixture,
    old: String,
    head: String,
    index: Vec<u8>,
    refs: Vec<u8>,
    status: Vec<u8>,
}
fn real(format: &str) -> Real {
    let fixture = Fixture::new();
    let repo = fixture.repo();
    git(
        &repo,
        &[
            "init",
            "--initial-branch=main",
            &format!("--object-format={format}"),
        ],
    );
    fs::write(repo.join("日本語.txt"), b"saved first\n").unwrap();
    git(&repo, &["add", "--", "日本語.txt"]);
    git(
        &repo,
        &[
            "commit",
            "--author=Original Writer <writer@example.invalid>",
            "-m",
            "Original saved source",
        ],
    );
    let old = sha(&repo, "HEAD");
    fs::write(repo.join("日本語.txt"), b"saved second\n").unwrap();
    git(&repo, &["add", "--", "日本語.txt"]);
    git(
        &repo,
        &[
            "commit",
            "--author=Later Writer <later@example.invalid>",
            "-m",
            "Later checkout source",
        ],
    );
    let head = sha(&repo, "HEAD");
    fs::write(repo.join("日本語.txt"), b"staged index bytes\n").unwrap();
    git(&repo, &["add", "--", "日本語.txt"]);
    fs::write(repo.join("日本語.txt"), b"dirty worktree bytes\n").unwrap();
    fs::write(repo.join("untracked.txt"), b"keep\n").unwrap();
    let index = fs::read(repo.join(".git/index")).unwrap();
    let refs = git(&repo, &["show-ref"]);
    let status = git(&repo, &["status", "--porcelain=v2", "-z"]);
    Real {
        fixture,
        old,
        head,
        index,
        refs,
        status,
    }
}
fn load(real: &Real, mode: RepositoryMode, revision: &str) -> Workspace {
    Workspace::load(&real.fixture.repo(), mode, revision, None)
        .unwrap()
        .unwrap()
}
fn unchanged(real: &Real) {
    let repo = real.fixture.repo();
    assert_eq!(sha(&repo, "HEAD"), real.head);
    assert_eq!(fs::read(repo.join(".git/index")).unwrap(), real.index);
    assert_eq!(git(&repo, &["show-ref"]), real.refs);
    assert_eq!(git(&repo, &["status", "--porcelain=v2", "-z"]), real.status);
    assert_eq!(
        fs::read(repo.join("日本語.txt")).unwrap(),
        b"dirty worktree bytes\n"
    );
    assert_eq!(fs::read(repo.join("untracked.txt")).unwrap(), b"keep\n");
}
fn compact(source: &PreparedRepositorySourceIdentity) -> String {
    source
        .compact()
        .iter()
        .map(|row| row.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
fn normalized(source: &PreparedRepositorySourceIdentity) -> String {
    source
        .detail_text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn repository_source_identity_all_seven_real_modes_distinguish_checkout_and_chosen_source() {
    let real = real("sha1");
    for (mode, _, _) in RepositoryMode::ALL {
        let workspace = load(&real, mode, &real.old);
        let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
        assert_eq!(source.checkout_head(), Some(real.head.as_str()));
        let expected = if matches!(
            mode,
            RepositoryMode::Graph | RepositoryMode::Tree | RepositoryMode::CommitChanges
        ) {
            real.old.as_str()
        } else {
            real.head.as_str()
        };
        assert_eq!(source.viewed_revision(), Some(expected));
        assert_eq!(source.actor_commit(), Some(expected));
        assert!(compact(&source).contains(&format!("HEAD: {}", real.head)));
        assert!(compact(&source).contains(&format!("Saved: {expected}")));
        assert_eq!(
            source.mutable(),
            matches!(
                mode,
                RepositoryMode::Overview | RepositoryMode::Working | RepositoryMode::Staged
            )
        );
        assert!(source
            .details()
            .iter()
            .all(|row| crate::editor::display_width(&row.text) <= 72));
    }
    unchanged(&real);
}
#[test]
fn repository_source_identity_working_and_staged_do_not_claim_immutable_targets() {
    let real = real("sha1");
    for (mode, expected) in [
        (RepositoryMode::Working, "Working: index"),
        (RepositoryMode::Staged, "Staged: observed saved HEAD"),
    ] {
        let workspace = load(&real, mode, &real.old);
        let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
        assert!(source.mutable());
        assert!(source.detail_text().contains(expected));
        assert!(
            source.detail_text().contains("mutable")
                && normalized(&source).contains("Neither role identifies who made current")
        );
        assert_eq!(source.actor_commit(), Some(real.head.as_str()));
        assert!(!source.detail_text().contains("Root starting content:"));
        let text = normalized(&source);
        assert!(!text.contains("Observed comparison B: unavailable"));
        assert!(!text.contains("Actual diff target object: unavailable"));
        assert!(text.contains("no saved object ID applies"));
        if mode == RepositoryMode::Working {
            assert!(text.contains("Observed comparison A: index (mutable"));
        } else {
            assert!(text.contains(&format!("Observed comparison A: {}", real.head)));
        }
    }
    unchanged(&real);
}
#[test]
fn repository_source_identity_real_sha256_full_head_and_chosen_ids_fit_narrow_rows() {
    let real = real("sha256");
    assert_eq!(real.head.len(), 64);
    let workspace = load(&real, RepositoryMode::Graph, &real.old);
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    assert_eq!(source.compact()[0].text, format!("HEAD: {}", real.head));
    assert_eq!(source.compact()[1].text, format!("Saved: {}", real.old));
    assert!(source
        .compact()
        .iter()
        .all(|row| crate::editor::display_width(&row.text) <= 72));
    assert!(source.detail_text().contains(&real.old) && source.detail_text().contains(&real.head));
    unchanged(&real);
}
#[test]
fn repository_source_identity_actor_basis_is_actual_displayed_saved_commit_not_origin_or_worktree()
{
    let real = real("sha1");
    let mut workspace = load(&real, RepositoryMode::Tree, &real.old);
    let actual = workspace.selected_commit.as_ref().unwrap().id.clone();
    workspace.revision = real.head.clone();
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    assert_eq!(source.viewed_revision(), Some(real.head.as_str()));
    assert_eq!(source.actor_commit(), Some(actual.as_str()));
    assert!(source
        .detail_text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains(&format!(
            "Displayed Git author and committer belong to saved commit: {actual}"
        )));
    let actor = workspace.selected_commit.as_ref().unwrap();
    assert_eq!(actor.author.name, "Original Writer");
    assert_eq!(actor.committer.name, "Distinct Integrator");
    unchanged(&real);
}
#[test]
fn repository_source_identity_working_search_preview_is_mutable_even_in_tree_mode() {
    let real = real("sha1");
    let mut workspace = Workspace::open_search_file(
        &real.fixture.repo(),
        &RawPath::new("日本語.txt".as_bytes()),
        None,
        1,
        CancellationToken::default(),
    )
    .unwrap();
    assert_eq!(workspace.mode, RepositoryMode::Overview);
    assert_eq!(workspace.preview.as_ref().unwrap().revision, "WORKTREE");
    for mode in [RepositoryMode::Overview, RepositoryMode::Tree] {
        workspace.mode = mode;
        let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
        assert!(source.mutable());
        assert!(source.viewed_revision().is_none());
        assert!(compact(&source).contains("working file; mutable"));
        assert!(normalized(&source).contains("WORKTREE content"));
        assert_eq!(source.checkout_head(), Some(real.head.as_str()));
    }
    unchanged(&real);
}
#[test]
fn repository_source_identity_snapshot_and_proposed_endpoints_use_actual_diff_start_not_requested_base(
) {
    let real = real("sha1");
    let repo = GitRepository::discover(real.fixture.repo())
        .unwrap()
        .unwrap();
    for kind in [
        Comparison::Snapshots {
            base: real.head.clone(),
            head: real.old.clone(),
        },
        Comparison::BranchChanges {
            base: real.head.clone(),
            head: real.old.clone(),
        },
    ] {
        let mut workspace = load(&real, RepositoryMode::Graph, &real.old);
        workspace.mode = RepositoryMode::CommitChanges;
        workspace.comparison = Some(repo.compare(&kind).unwrap());
        let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
        assert!(!source.mutable());
        let normalized = source
            .detail_text()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            normalized.contains(&format!("Observed comparison A: {}", real.head))
                && normalized.contains(&format!("Observed comparison B: {}", real.old))
        );
        if matches!(kind, Comparison::BranchChanges { .. }) {
            assert!(normalized.contains(&format!("Common starting point: {}", real.old)));
            assert!(normalized.contains(&format!("Actual diff starting object: {}", real.old)));
            assert!(normalized.contains("Direction matters"));
        } else {
            assert!(normalized.contains(&format!("Actual diff starting object: {}", real.head)));
            assert!(normalized.contains("complete A-to-B"));
        }
    }
    unchanged(&real);
}
#[test]
fn repository_source_identity_genuine_root_empty_tree_is_not_a_parent_commit() {
    let real = real("sha1");
    let workspace = load(&real, RepositoryMode::CommitChanges, &real.old);
    assert!(workspace
        .selected_commit
        .as_ref()
        .unwrap()
        .parents
        .is_empty());
    let comparison = workspace.comparison.as_ref().unwrap();
    assert_ne!(comparison.base, comparison.head);
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    let normalized = source
        .detail_text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(normalized
        .contains("Root starting content: the genuine loaded original commit has no parents"));
    assert!(normalized.contains("empty starting tree, not a parent commit"));
    assert!(normalized.contains(comparison.base.as_ref().unwrap()));
    unchanged(&real);
}
#[test]
fn repository_source_identity_missing_or_shallow_parent_never_invents_root_tree() {
    let real = real("sha1");
    let root = load(&real, RepositoryMode::CommitChanges, &real.old);
    for changed in 0..7 {
        let mut workspace = root.clone();
        match changed {
            0 => workspace.comparison.as_mut().unwrap().base = None,
            1 => workspace.comparison.as_mut().unwrap().head = None,
            2 => workspace.selected_commit = None,
            3 => workspace.status.shallow = true,
            4 => workspace.comparison.as_mut().unwrap().diff.base = None,
            5 => {
                workspace.comparison.as_mut().unwrap().comparison = Comparison::CommitChanges {
                    commit: real.head.clone(),
                    parent: 0,
                }
            }
            _ => {
                let comparison = workspace.comparison.as_mut().unwrap();
                comparison.base = comparison.head.clone();
                comparison.diff.base = comparison.head.clone();
            }
        }
        let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
        assert!(!source.detail_text().contains("Root starting content:"));
        if changed == 3 {
            assert!(source.detail_text().contains("Shallow history:"));
        }
    }
    unchanged(&real);
}
#[test]
fn repository_source_identity_real_parent_selection_is_not_root_absence_or_checkout() {
    let real = real("sha1");
    let workspace = load(&real, RepositoryMode::CommitChanges, &real.head);
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    assert!(source
        .detail_text()
        .contains(&format!("Selected parent 1: {}", real.old)));
    assert!(!source.detail_text().contains("Root starting content:"));
    assert!(normalized(&source).contains("does not check it out"));
    unchanged(&real);
}
#[test]
fn repository_source_identity_overrides_keep_original_snapshot_limitations_visible() {
    let real = real("sha1");
    let mut workspace = load(&real, RepositoryMode::Graph, &real.old);
    workspace.status.replacement_refs = 2;
    workspace.status.grafted = true;
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    let text = source
        .detail_text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains("replacement refs 2")
            && text.contains("legacy grafts present")
            && text.contains("Original snapshot limitation")
    );
    assert_eq!(source.viewed_revision(), Some(real.old.as_str()));
    unchanged(&real);
}
#[test]
fn repository_source_identity_unborn_detached_and_missing_actor_are_distinct_honest_states() {
    let fixture = Fixture::new();
    git(&fixture.repo(), &["init", "--initial-branch=main"]);
    fs::write(fixture.repo().join("first.txt"), b"initial\n").unwrap();
    git(&fixture.repo(), &["add", "first.txt"]);
    let workspace = Workspace::load(&fixture.repo(), RepositoryMode::Staged, "HEAD", None)
        .unwrap()
        .unwrap();
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    assert!(source.checkout_head().is_none() && source.actor_commit().is_none());
    assert!(source.mutable());
    assert!(!source.detail_text().contains("Root starting content:"));
    let real = real("sha1");
    git(&real.fixture.repo(), &["checkout", "--detach", &real.head]);
    let mut workspace = load(&real, RepositoryMode::Graph, &real.old);
    workspace.selected_commit = None;
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    assert!(source.detail_text().contains("detached HEAD"));
    assert!(source.checkout_head().is_some());
    assert!(normalized(&source).contains("does not prove the repository has no saved versions"));
    assert!(source.actor_commit().is_none());
}
#[test]
fn repository_source_identity_invalid_short_and_zero_sha_do_not_resolve_or_substitute() {
    let real = real("sha1");
    let mut workspace = load(&real, RepositoryMode::Graph, &real.old);
    workspace.status.head = Some("abc1234".into());
    workspace.revision = "a".repeat(40);
    workspace.selected_commit.as_mut().unwrap().id = "0".repeat(40);
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    assert!(source.checkout_head().is_none() && source.actor_commit().is_none());
    assert_eq!(source.viewed_revision(), Some("a".repeat(40).as_str()));
    assert!(
        compact(&source).contains("HEAD: unavailable")
            && compact(&source).contains("Saved: unavailable")
    );
    unchanged(&real);
}
#[test]
fn repository_source_identity_raw_reference_controls_unicode_and_oversize_preflight() {
    let real = real("sha1");
    let mut workspace = load(&real, RepositoryMode::Graph, &real.old);
    workspace.status.raw_branch = Some("界\u{202e}\u{1b}[31m".as_bytes().to_vec());
    let source = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    assert!(source.detail_text().contains('界'));
    assert!(!source.detail_text().contains('\u{202e}') && !source.detail_text().contains('\u{1b}'));
    assert!(source.detail_text().contains("\\u{202e}\\u{1b}[31m"));
    let escaped = source.detail_text().to_owned();
    workspace.status.raw_branch = Some(b"literal\\u{202e}\\u{1b}[31m".to_vec());
    let literal = PreparedRepositorySourceIdentity::prepare(&workspace).unwrap();
    assert!(literal
        .detail_text()
        .contains("literal\\\\u{202e}\\\\u{1b}[31m"));
    assert_ne!(literal.detail_text(), escaped);
    for changed in [0, 1, 2, 3] {
        let mut bad = workspace.clone();
        match changed {
            0 => bad.revision = "x".repeat(MAX_FIELD_BYTES + 1),
            1 => bad.status.raw_branch = Some(vec![b'x'; MAX_FIELD_BYTES + 1]),
            2 => {
                bad.comparison = Some(
                    GitRepository::discover(real.fixture.repo())
                        .unwrap()
                        .unwrap()
                        .compare(&Comparison::Snapshots {
                            base: real.old.clone(),
                            head: real.head.clone(),
                        })
                        .unwrap(),
                );
                bad.comparison.as_mut().unwrap().base = Some("x".repeat(MAX_FIELD_BYTES + 1));
            }
            _ => {
                bad = load(&real, RepositoryMode::CommitChanges, &real.head);
                bad.selected_commit.as_mut().unwrap().parents[0] = "x".repeat(MAX_FIELD_BYTES + 1);
            }
        }
        assert!(PreparedRepositorySourceIdentity::prepare(&bad).is_err());
    }
    unchanged(&real);
}
#[test]
fn repository_source_identity_zero_width_graphemes_and_rows_have_hard_byte_limits() {
    let mut rows = Rows::default();
    assert!(rows
        .text(
            &format!("e{}", "\u{301}".repeat(600)),
            SemanticToken::Normal
        )
        .is_err());
    let mut rows = Rows::default();
    for _ in 0..MAX_ROWS {
        rows.row("".into(), SemanticToken::Normal).unwrap();
    }
    assert!(rows.row("".into(), SemanticToken::Normal).is_err());
    let mut rows = Rows::default();
    assert!(rows.raw(&vec![0; MAX_FIELD_BYTES + 1]).is_err());
    assert!(rows.rows.is_empty());
}
#[test]
fn repository_source_identity_no_color_labels_full_sha_and_arc_clone_are_prepared() {
    let real = real("sha1");
    let source =
        PreparedRepositorySourceIdentity::prepare(&load(&real, RepositoryMode::Graph, &real.old))
            .unwrap();
    let clone = source.clone();
    assert!(
        Arc::ptr_eq(&source.compact, &clone.compact)
            && Arc::ptr_eq(&source.details, &clone.details)
    );
    let theme = crate::theme::Theme::new(
        crate::config::ThemeName::NoColor,
        crate::theme::ColorMode::NoColor,
    );
    assert!(theme.style(source.compact()[0].role).fg.is_none());
    assert!(source.compact()[0].text.starts_with("HEAD:"));
    assert!(source.compact()[1].text.starts_with("Saved:"));
    for row in source.compact().iter().chain(source.details()) {
        assert!(crate::editor::display_width(&row.text) <= 72 && row.text.len() <= MAX_ROW_BYTES);
    }
    unchanged(&real);
}
