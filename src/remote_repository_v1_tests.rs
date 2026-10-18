//! UNRUN worker/model contracts. Real provider observations, no fake proof DTO.
use super::*;
use crate::{
    providers::runner::CancellationToken,
    remote_repository_v1_test_support as fx,
    storage::Cache,
    test_support::{Fixture, GhStep},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};
fn loaded(
    fixture: &Fixture,
    objects: &fx::Objects,
) -> (
    Arc<crate::storage::CatalogueIntentSource>,
    Arc<RemoteRepositoryOwnerV1>,
    RemoteRepositoryWorkspaceV1,
    Vec<GhStep>,
) {
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    steps.extend(fx::default_payload(objects));
    steps.extend(fx::suffix(42));
    fixture.scripted_gh(&steps);
    let source = fx::source(fixture);
    let owner = RemoteRepositoryOwnerV1::from_row(source.source().clone(), 0).unwrap();
    let result = read_remote_repository_v1(
        RemoteRepositoryEnvironmentV1 {
            provider: &fx::provider(fixture),
            database: &fixture.root.join("cache.sqlite3"),
        },
        source.clone(),
        owner.clone(),
        7,
        RemoteRepositoryRequestV1::DefaultTree,
        &Default::default(),
        &Default::default(),
    );
    let RemoteRepositoryPreparedV1::Ready { workspace, .. } = result else {
        panic!("fresh actual directory missing");
    };
    (source, owner, *workspace, steps)
}
#[test]
fn remote_tree_v1_current_source_and_original_row_indices_survive_keyboard_mouse_three_widths() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let (source, owner, mut ws, steps) = loaded(&fixture, &objects);
    for width in [120, 80, 40] {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|frame| {
                ws.draw(
                    frame,
                    frame.area(),
                    crate::theme::Theme::new(
                        crate::config::ThemeName::CatppuccinMocha,
                        crate::theme::ColorMode::TrueColor,
                    ),
                )
            })
            .unwrap();
        for action in [
            RemoteBrowseActionV1::Open,
            RemoteBrowseActionV1::Branches,
            RemoteBrowseActionV1::Tags,
            RemoteBrowseActionV1::CommitFocus,
            RemoteBrowseActionV1::MappedLocal,
            RemoteBrowseActionV1::Details,
            RemoteBrowseActionV1::Back,
        ] {
            let (rect, _) = ws
                .hits
                .iter()
                .find(|(_, candidate)| candidate == &action)
                .unwrap();
            assert!(rect.width > 0 && rect.right() <= width);
            assert_eq!(
                ws.mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x,
                    row: rect.y,
                    modifiers: KeyModifiers::NONE
                }),
                Some(action)
            );
        }
        assert_eq!(
            ws.key(KeyEvent::new(KeyCode::Char('B'), KeyModifiers::NONE)),
            Some(RemoteBrowseActionV1::Branches)
        );
        let row = ws
            .rows
            .iter()
            .position(|r| r.label.contains("nested"))
            .unwrap();
        ws.local(&RemoteBrowseActionV1::Row(row)).unwrap();
        assert!(
            matches!(ws.request(&RemoteBrowseActionV1::Open).unwrap(),Some(RemoteRepositoryRequestV1::Directory{index,..})if index==ws.rows[row].index)
        );
    }
    assert!(Arc::ptr_eq(&ws.owner, &owner));
    Cache::open(&fixture.root.join("cache.sqlite3"))
        .unwrap()
        .validate_repository_catalogue_intent_source(&source)
        .unwrap();
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_equal_text_owner_and_generation_do_not_admit_late_completion() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let (source, owner, ws, steps) = loaded(&fixture, &objects);
    let other = RemoteRepositoryOwnerV1::from_row(source.source().clone(), 0).unwrap();
    let result = RemoteRepositoryPreparedV1::Ready {
        owner: owner.clone(),
        source: source.clone(),
        generation: 7,
        workspace: Box::new(ws),
    };
    assert!(owns_remote_repository_result_v1(
        Some(&owner),
        Some(&source),
        7,
        &result
    ));
    assert!(!owns_remote_repository_result_v1(
        Some(&other),
        Some(&source),
        7,
        &result
    ));
    assert!(!owns_remote_repository_result_v1(
        Some(&owner),
        Some(&source),
        8,
        &result
    ));
    assert!(!owns_remote_repository_result_v1(
        None,
        Some(&source),
        7,
        &result
    ));
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_partial_membership_disables_open_and_exact_parent_reload_is_explicit() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    let mut payload = fx::default_payload(&objects);
    let mut partial = objects.root_json.clone();
    partial["truncated"] = serde_json::json!(true);
    partial["tree"].as_array_mut().unwrap().truncate(1);
    payload[2] = fx::get(
        &format!("repos/example/remote/git/trees/{}", objects.tree),
        partial,
    );
    steps.extend(payload);
    steps.extend(fx::suffix(42));
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let owner = RemoteRepositoryOwnerV1::from_row(source.source().clone(), 0).unwrap();
    let result = read_remote_repository_v1(
        RemoteRepositoryEnvironmentV1 {
            provider: &fx::provider(&fixture),
            database: &fixture.root.join("cache.sqlite3"),
        },
        source,
        owner,
        1,
        RemoteRepositoryRequestV1::DefaultTree,
        &Default::default(),
        &Default::default(),
    );
    let RemoteRepositoryPreparedV1::Ready { workspace, .. } = result else {
        panic!("reported partial should remain labelled");
    };
    assert!(workspace
        .header
        .iter()
        .any(|s| s.contains("Partial reported")));
    assert!(workspace.request(&RemoteBrowseActionV1::Open).is_err());
    assert!(matches!(
        workspace.request(&RemoteBrowseActionV1::Refresh).unwrap(),
        Some(RemoteRepositoryRequestV1::ReloadDirectory { .. })
    ));
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_original_sql_change_is_rejected_before_payload_no_latest_generation_adoption() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let owner = RemoteRepositoryOwnerV1::from_row(source.source().clone(), 0).unwrap();
    Cache::open(&fixture.root.join("cache.sqlite3"))
        .unwrap()
        .save_view(
            source.scope(),
            source.saved_view_key(),
            &serde_json::json!({"foreign":"PRIVATE_SQL_CANARY"}),
        )
        .unwrap();
    let result = read_remote_repository_v1(
        RemoteRepositoryEnvironmentV1 {
            provider: &fx::provider(&fixture),
            database: &fixture.root.join("cache.sqlite3"),
        },
        source,
        owner,
        1,
        RemoteRepositoryRequestV1::DefaultTree,
        &Default::default(),
        &Default::default(),
    );
    assert!(matches!(
        result,
        RemoteRepositoryPreparedV1::Hidden {
            scope_unavailable: true,
            ..
        }
    ));
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_cancel_before_worker_never_opens_private_database_or_queries() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let steps = fx::source_steps();
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let owner = RemoteRepositoryOwnerV1::from_row(source.source().clone(), 0).unwrap();
    let token = CancellationToken::default();
    token.cancel();
    let cold = fixture.root.join("cold/no-sql.sqlite3");
    assert!(matches!(
        read_remote_repository_v1(
            RemoteRepositoryEnvironmentV1 {
                provider: &fx::provider(&fixture),
                database: &cold
            },
            source,
            owner,
            1,
            RemoteRepositoryRequestV1::DefaultTree,
            &Default::default(),
            &token
        ),
        RemoteRepositoryPreparedV1::Hidden {
            scope_unavailable: true,
            ..
        }
    ));
    assert!(!cold.parent().unwrap().exists());
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}

#[test]
fn remote_tree_v1_worker_prepares_exact_unicode_blob_and_fresh_confirm_generation() {
    use crate::search::{
        corpus::LocalSource,
        managed::{CorpusOwnership, CorpusRepository},
    };
    use std::os::unix::ffi::OsStrExt;
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    steps.extend(fx::default_payload(&objects));
    steps.extend(fx::suffix(42));
    steps.extend(fx::prefix());
    steps.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", objects.child),
        objects.child_json.clone(),
    ));
    steps.extend(fx::suffix(42));
    steps.extend(fx::prefix());
    steps.push(fx::blob_step(&objects));
    steps.extend(fx::suffix(42));
    for _ in 0..2 {
        steps.extend(fx::prefix());
        steps.extend(fx::suffix(42));
    }
    assert_eq!(steps.len(), 43);
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let owner = RemoteRepositoryOwnerV1::from_row(source.source().clone(), 0).unwrap();
    let provider = fx::provider(&fixture);
    let database = fixture.root.join("cache.sqlite3");
    let limits = Default::default();
    let cancel = Default::default();
    let read = |request| {
        read_remote_repository_v1(
            RemoteRepositoryEnvironmentV1 {
                provider: &provider,
                database: &database,
            },
            source.clone(),
            owner.clone(),
            7,
            request,
            &limits,
            &cancel,
        )
    };
    let RemoteRepositoryPreparedV1::Ready { workspace, .. } =
        read(RemoteRepositoryRequestV1::DefaultTree)
    else {
        panic!("genuine root")
    };
    let RemoteBrowsePageV1::Directory(root) = workspace.page else {
        panic!("root directory")
    };
    let index = root
        .entries()
        .iter()
        .position(|entry| entry.name() == "nested")
        .unwrap();
    let RemoteRepositoryPreparedV1::Ready { workspace, .. } =
        read(RemoteRepositoryRequestV1::Directory {
            original: root,
            index,
        })
    else {
        panic!("genuine nested directory")
    };
    let RemoteBrowsePageV1::Directory(child) = workspace.page else {
        panic!("nested directory")
    };
    let RemoteRepositoryPreparedV1::Ready { workspace, .. } =
        read(RemoteRepositoryRequestV1::File {
            original: child,
            index: 0,
        })
    else {
        panic!("genuine immutable file")
    };
    let RemoteBrowsePageV1::File { original, preview } = workspace.page else {
        panic!("exact file preview")
    };
    assert_eq!(original.entry().oid(), objects.blob);
    assert_eq!(original.bytes(), objects.bytes);
    assert_eq!(preview.revision, objects.commit);
    assert!(preview
        .rows
        .iter()
        .any(|row| row.label.contains("old remote saved content")));
    assert!(preview.rows.iter().any(|row| row.label.contains("日本語")));
    let mapping = serde_json::to_value(CorpusRepository {
        scope: source.scope().clone(),
        repository: "example/remote".into(),
        directory: RawPath(fixture.repo().as_os_str().as_bytes().to_vec()),
        source: LocalSource::Revision(objects.commit.clone()),
        ownership: CorpusOwnership::Existing,
        active: true,
        saved_commit: Some(objects.commit.clone()),
        disk_bytes: 0,
        disk_complete: false,
        clone_source: None,
    })
    .unwrap();
    let key = crate::search::managed::mapping_key("example/remote");
    Cache::open(&database)
        .unwrap()
        .save_view(source.scope(), &key, &mapping)
        .unwrap();
    let preview = preview_remote_local_handoff_v1(
        RemoteRepositoryEnvironmentV1 {
            provider: &provider,
            database: &database,
        },
        source.clone(),
        owner.clone(),
        8,
        original,
        &limits,
        &cancel,
    )
    .unwrap();
    let confirmed =
        confirm_remote_local_handoff_v1(&provider, &database, &preview, 9, &limits, &cancel)
            .unwrap();
    assert_eq!(
        confirmed.generation, 9,
        "the new confirm owns its current operation, not the old preview generation"
    );
    assert_eq!(preview.generation(), 8);
    assert_eq!(confirmed.identity.account_id, Some(42));
    assert!(Arc::ptr_eq(&confirmed.source, &source));
    assert!(Arc::ptr_eq(&confirmed.owner, &owner));
    assert_eq!(confirmed.workspace.revision, objects.commit);
    assert_eq!(
        confirmed.workspace.preview.as_ref().unwrap().path,
        RawPath("nested/日本語.txt".as_bytes().to_vec())
    );
    let cache = Cache::open(&database).unwrap();
    assert_eq!(
        cache.saved_view(source.scope(), &key).unwrap(),
        Some(mapping)
    );
    cache
        .validate_repository_catalogue_intent_source(&source)
        .unwrap();
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
