//! Real-object/local-role contracts; no gh/current remote authority.
use super::*;
use crate::{
    config::ThemeName,
    git::GitRepository,
    identity::ActorWidget,
    theme::{ColorMode, Theme},
};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn actor(name: &str, email: &str) -> GitActor {
    GitActor {
        name: name.into(),
        email: email.into(),
        timestamp: 1_700_000_000,
    }
}
fn commit(author: GitActor, committer: GitActor) -> Commit {
    Commit {
        id: "a".repeat(40),
        parents: vec![],
        author,
        committer,
        subject: "Original saved change".into(),
        refs: vec![],
        raw_refs: vec![],
    }
}
#[test]
fn git_actor_roles_known_metadata_stays_exact_and_roles_remain_distinct() {
    let source = actor("Observed Writer", "writer@example.invalid");
    for role in [GitRole::Author, GitRole::Committer] {
        let display = display_identity(&source, role);
        assert_eq!(display.name, source.name);
        assert_eq!(display.email.as_deref(), Some(source.email.as_str()));
        assert_eq!(display.git_role, Some(role));
        assert!(!display.host_associated);
        assert_eq!(display.login, None);
        assert_eq!(display.avatar_url, None);
        assert!(display.label().contains("[local Git]"));
    }
    assert_ne!(
        display_identity(&source, GitRole::Author),
        display_identity(&source, GitRole::Committer)
    );
}
#[test]
fn git_actor_roles_empty_and_whitespace_name_email_are_honest_unknown() {
    for (name, email) in [
        ("", ""),
        ("   ", " "),
        ("", "known@example.invalid"),
        ("Known Name", ""),
    ] {
        let source = actor(name, email);
        let label = local_label(name, Some(email), Some(GitRole::Author));
        assert!(label.starts_with("author "));
        if name.trim().is_empty() {
            assert!(label.contains("unknown name"));
            assert_eq!(local_initials(name), "?")
        }
        if email.trim().is_empty() {
            assert!(label.contains("email unavailable"));
            assert!(!label.contains("<>"))
        }
        assert_eq!(source.name, name);
        assert_eq!(source.email, email);
    }
}
#[test]
fn git_actor_roles_known_local_ghost_or_login_like_name_is_not_github_user() {
    for name in ["ghost", "octocat", "@alice"] {
        let identity = display_identity(&actor(name, "local@example.invalid"), GitRole::Author);
        assert_eq!(identity.kind, ActorKind::Local);
        assert_eq!(identity.host, None);
        assert_eq!(identity.login, None);
        assert!(!identity.label().contains("[deleted]"));
        assert!(identity.label().contains("[local Git]"));
    }
}
#[test]
fn git_actor_roles_oversized_field_refuses_central_copy_preserves_role_and_source() {
    for role in [GitRole::Author, GitRole::Committer] {
        let source = actor(&"x".repeat(FIELD_BYTES + 1), "actual@example.invalid");
        let identity = ActorIdentity::local(&source, role);
        assert_eq!(source.name.len(), FIELD_BYTES + 1);
        assert!(identity.name.len() < 128);
        assert_eq!(identity.kind, ActorKind::Unknown);
        assert_eq!(identity.git_role, Some(role));
        assert!(identity.label().contains("display limit"));
        assert!(identity.label().starts_with(role_label(Some(role))));
        assert_eq!(identity.initials(), "?");
        assert_eq!(identity.avatar_url, None);
    }
}
#[test]
fn git_actor_roles_boundary_field_never_truncates_to_a_different_observed_name() {
    let source = actor(&"x".repeat(FIELD_BYTES), &"y".repeat(FIELD_BYTES));
    let identity = display_identity(&source, GitRole::Committer);
    assert_eq!(identity.name, source.name);
    assert_eq!(identity.email.as_deref(), Some(source.email.as_str()));
    let too_large = actor("Known", &"x".repeat(FIELD_BYTES + 1));
    assert!(actor_text(&too_large).contains("display limit"));
    assert_eq!(too_large.email.len(), FIELD_BYTES + 1);
}
#[test]
fn git_actor_roles_unsafe_direct_metadata_is_unavailable_not_terminal_controls() {
    for name in ["unsafe\x1b[31m", "line\nname", "bidi\u{202e}name"] {
        let source = actor(name, "known@example.invalid");
        let identity = display_identity(&source, GitRole::Author);
        assert_eq!(identity.kind, ActorKind::Unknown);
        assert!(!identity.label().contains('\x1b'));
        assert!(!identity.label().contains('\n'));
        assert!(!identity.label().contains('\u{202e}'));
        assert_eq!(source.name, name);
    }
}
#[test]
fn git_actor_roles_initials_are_bounded_two_cells_including_wide_no_name_and_combining() {
    for name in ["Observed Writer", "太郎", "e\u{301}", "ß", " \t", ""] {
        let initials = local_initials(name);
        assert!(ratatui::text::Span::raw(initials.as_str()).width() <= 2);
        assert!(!initials.is_empty());
    }
    assert_eq!(local_initials("太郎"), "太");
    assert_eq!(local_initials("Observed Writer"), "OW");
    assert_eq!(local_initials(""), "?");
}
#[test]
fn git_actor_roles_commit_summary_and_graph_show_both_actual_roles_not_implicit_author() {
    let source = commit(
        actor("Writer", "author@example.invalid"),
        actor("Recorder", "committer@example.invalid"),
    );
    assert_eq!(
        commit_summary(&source),
        "author Writer • committer Recorder • 2023-11-14"
    );
    let label = commit_summary(&source);
    assert!(label.contains("author Writer"));
    assert!(label.contains("committer Recorder"));
    assert_eq!(source.id, "a".repeat(40));
}
#[test]
fn git_actor_roles_no_color_unknown_author_and_committer_keep_readable_role_text() {
    for role in [GitRole::Author, GitRole::Committer] {
        let identity = display_identity(&actor("", ""), role);
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 1));
        ActorWidget {
            identity: &identity,
            image: None,
            theme: Theme::new(ThemeName::CatppuccinMocha, ColorMode::NoColor),
        }
        .render(buffer.area, &mut buffer);
        let text = buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains(role_label(Some(role)).trim()));
        assert!(text.contains("unknown name"));
        assert!(text.contains("email unavailable"));
    }
}
fn git(root: &std::path::Path, args: &[&str], input: Option<&str>) -> String {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Actual installed Git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
#[test]
fn git_actor_roles_real_sha1_and_sha256_commit_objects_preserve_missing_fields() {
    for format in ["sha1", "sha256"] {
        let dir = tempfile::tempdir().unwrap();
        git(
            dir.path(),
            &[
                "init",
                "-q",
                "--initial-branch=main",
                &format!("--object-format={format}"),
            ],
            None,
        );
        let tree = git(
            dir.path(),
            &["hash-object", "-t", "tree", "-w", "--stdin"],
            Some(""),
        );
        let raw=format!("tree {tree}\nauthor  <> 1700000000 +0000\ncommitter Actual Recorder <record@example.invalid> 1700000100 +0000\n\nUnknown author saved version\n");
        let id = git(
            dir.path(),
            &["hash-object", "-t", "commit", "-w", "--stdin"],
            Some(&raw),
        );
        git(dir.path(), &["update-ref", "refs/heads/main", &id], None);
        let repo = GitRepository::discover(dir.path()).unwrap().unwrap();
        let source = repo.log_page(&id, 0, 1).unwrap().remove(0);
        assert_eq!(source.id, id);
        assert_eq!(source.id.len(), if format == "sha1" { 40 } else { 64 });
        assert_eq!(source.author.name, "");
        assert_eq!(source.author.email, "");
        assert_eq!(source.committer.name, "Actual Recorder");
        assert!(commit_summary(&source).contains("author unknown name [email unavailable]"));
        assert!(commit_summary(&source).contains("committer Actual Recorder"));
        assert_eq!(
            git(dir.path(), &["cat-file", "commit", &id], None),
            raw.trim()
        );
    }
}
#[test]
fn v1_repo_views_commit_byline_names_one_actor_once_with_a_readable_date() {
    let same = commit(
        actor("Writer", "author@example.invalid"),
        actor("Writer", "author@example.invalid"),
    );
    assert_eq!(commit_summary(&same), "Writer • 2023-11-14");
    let unnamed = commit(actor("", ""), actor("", ""));
    assert_eq!(
        commit_summary(&unnamed),
        "unknown name [email unavailable] • 2023-11-14"
    );
    assert_eq!(utc_date(0), "1970-01-01");
    assert_eq!(utc_date(951_782_400), "2000-02-29");
    assert_eq!(utc_date(-86_400), "1969-12-31");
    assert_eq!(utc_datetime(1_700_000_000), "2023-11-14 22:13 UTC");
}
