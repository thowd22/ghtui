//! Judged lexical search and derivative recovery through the actual executable/input paths.
use crate::support::{Fixture, Session};
use ghtui::storage::{Cache, Scope};

fn choose(session: &mut Session, mouse: bool, label: &str, keys: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(keys);
    }
}
fn query(session: &mut Session, mouse: bool, value: &str) {
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste(value);
    choose(session, mouse, "[Enter Search]", b"\r");
}
fn ranked(session: &mut Session, mouse: bool) {
    session.wait_for_screen("Recent commits");
    choose(session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    choose(session, mouse, "n Ranked", b"n");
}
fn judged_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "src/AuthBoundary.rs",
        "你好 AuthBoundary old saved version\n".as_bytes(),
        "Saved auth implementation",
    );
    fixture.commit_file(
        "docs/guide.md",
        b"auth boundary overview explains architecture\n",
        "Architecture notes",
    );
    fixture.commit_file(
        "src/noise.rs",
        b"boundary distant words auth\n",
        "Different code",
    );
    std::fs::write(
        fixture.repo().join("src/AuthBoundary.rs"),
        "你好 AuthBoundary keeps safe state\nworking version to preserve\n",
    )
    .unwrap();
    fixture
}
fn judged_journey(mouse: bool) {
    let fixture = judged_fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let working = std::fs::read(fixture.repo().join("src/AuthBoundary.rs")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    ranked(&mut session, mouse);
    query(&mut session, mouse, "AuthBoundary");
    session.wait_for_screen("2 results • Ranked");
    session.wait_for_selected_cells(
        &crate::support::view_title("Search"),
        &["src/AuthBoundary.rs", " 1:4 "],
    );
    session.wait_for_screen("Why:");
    session.wait_for_screen("title lexical tokens");
    query(&mut session, mouse, "\"keeps safe state\"");
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_row(&["src/AuthBoundary.rs", " 1:17 "]);
    session.wait_for_screen("body lexical tokens");
    query(&mut session, mouse, "path:src/ AuthBoundary");
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_selected_cells(
        &crate::support::view_title("Search"),
        &["src/AuthBoundary.rs", " 1:4 "],
    );
    session.resize(80, 24);
    session.wait_for_screen("Why:");
    session.wait_for_screen("keeps safe state");
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Opened exact search destination");
    session.wait_for_screen("working files • mutable");
    session.wait_for_screen("你好 AuthBoundary keeps safe state");
    assert!(!session.screen().contains("old saved version"));
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let documents = cache
        .list(
            &Scope::new("local", "workspace").unwrap(),
            Some("search-index-document-v1"),
        )
        .unwrap();
    assert_eq!(documents.len(), 3);
    let source = documents
        .iter()
        .find(|record| record.payload["title"] == "src/AuthBoundary.rs")
        .unwrap();
    assert_eq!(source.payload["key"]["revision"], "working");
    assert_eq!(source.payload["body"].as_str().unwrap().as_bytes(), working);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("src/AuthBoundary.rs")).unwrap(),
        working
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn search_relevance_keyboard_judged_identifier_phrase_path_explanations_and_exact_open() {
    judged_journey(false);
}
#[test]
fn search_relevance_mouse_judged_identifier_phrase_path_explanations_and_exact_open() {
    judged_journey(true);
}

fn corrupt_journey(mouse: bool) {
    let fixture = judged_fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let working = std::fs::read(fixture.repo().join("src/AuthBoundary.rs")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    ranked(&mut session, mouse);
    query(&mut session, mouse, "path:src/ AuthBoundary");
    session.wait_for_screen("1 results • Ranked");
    session.quit_and_assert_restored_terminal(mouse);
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let scope = Scope::new("local", "workspace").unwrap();
    let cache = Cache::open(&database).unwrap();
    let original = cache
        .list(&scope, Some("search-index-document-v1"))
        .unwrap();
    assert_eq!(original.len(), 3);
    cache
        .put_draft(
            &scope,
            "unrelated-review",
            "draft survives derivative recovery",
        )
        .unwrap();
    let index_root = fixture.root.join("config/ghtui/index");
    let owned = std::fs::read_dir(&index_root)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path.join("ghtui-owner.json").is_file())
        .unwrap();
    let damaged = b"invalid metadata from an offline disposable corruption fixture";
    std::fs::write(owned.join("meta.json"), damaged).unwrap();
    drop(cache);
    let mut session = Session::launch(&fixture, 80, 24);
    ranked(&mut session, mouse);
    query(&mut session, mouse, "path:src/ AuthBoundary");
    session.wait_for_screen("1 results • Ranked");
    session.wait_for_screen("REBUILT ranked index from SQLite");
    session.wait_for_screen("keeps safe state");
    let cache = Cache::open(&database).unwrap();
    let current = cache
        .list(&scope, Some("search-index-document-v1"))
        .unwrap();
    assert_eq!(current.len(), original.len());
    for source in original {
        assert_eq!(
            current
                .iter()
                .find(|record| record.id == source.id)
                .unwrap()
                .payload["body"],
            source.payload["body"]
        );
    }
    assert_eq!(
        cache.draft(&scope, "unrelated-review").unwrap().as_deref(),
        Some("draft survives derivative recovery")
    );
    let backup = std::fs::read_dir(&index_root)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(".recovery-")
        })
        .unwrap();
    assert_eq!(std::fs::read(backup.join("meta.json")).unwrap(), damaged);
    choose(&mut session, mouse, "[o Open result]", b"o");
    session.wait_for_screen("Opened exact search destination");
    session.wait_for_screen("你好 AuthBoundary keeps safe state");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("src/AuthBoundary.rs")).unwrap(),
        working
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn search_index_keyboard_corrupt_derivative_rebuild_is_visible_and_preserves_sources_drafts() {
    corrupt_journey(false);
}
#[test]
fn search_index_mouse_corrupt_derivative_rebuild_is_visible_and_preserves_sources_drafts() {
    corrupt_journey(true);
}
