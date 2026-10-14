//! SOURCE-ONLY/UNREGISTERED/UNRUN exact remote read cases. No proof constructors.
use super::*;
use model::providers::gh::{GithubError, GithubErrorKind, RemoteRepositoryReadPhaseV1};

fn named(root: &model::providers::gh::RemoteDirectoryV1, name: &str) -> usize {
    root.entries()
        .iter()
        .position(|row| row.name() == name)
        .unwrap()
}
fn root_blob(fixture: &Fixture, objects: &fx::Objects, name: &str) -> Vec<GhStep> {
    let row = objects.root_json["tree"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["path"] == name)
        .unwrap();
    let oid = row["sha"].as_str().unwrap();
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/example/remote/git/blobs/{oid}"),
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--allow-escape-sequences",
        ],
        "",
    );
    step.stdout = fx::git(&fixture.repo(), &["cat-file", "blob", oid]);
    let mut result = fx::prefix();
    result.push(step);
    result.extend(fx::suffix(42));
    result
}
#[test]
fn remote_tree_v1_followup_provider_exact_manual_commit_never_resolves_live_head_or_ref() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    steps.push(fx::get(
        &format!("repos/example/remote/git/commits/{}", objects.commit),
        json!({"sha":objects.commit,"tree":{"sha":objects.tree}}),
    ));
    steps.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", objects.tree),
        objects.root_json.clone(),
    ));
    steps.extend(fx::suffix(42));
    assert_eq!(steps.len(), 15);
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let actual = dir(read(&fixture, &source, Request::Commit(objects.commit.clone())).unwrap());
    assert_eq!(actual.commit(), objects.commit);
    assert_eq!(actual.tree(), objects.tree);
    assert!(actual.complete());
    assert_ne!(
        String::from_utf8(fx::git(&fixture.repo(), &["rev-parse", "HEAD"]))
            .unwrap()
            .trim(),
        actual.commit()
    );
    assert!(steps
        .iter()
        .all(|step| !step.args.iter().any(|arg| arg.contains("git/ref/"))));
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_followup_provider_binary_lfs_and_symlink_are_verified_literal_blobs() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(default_steps(&objects));
    for name in ["binary.bin", "lfs.txt", "link"] {
        steps.extend(root_blob(&fixture, &objects, name));
    }
    assert_eq!(steps.len(), 38);
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let original = dir(read(&fixture, &source, Request::DefaultTree).unwrap());
    for (name, kind) in [
        ("binary.bin", model::providers::gh::RemoteEntryKindV1::File),
        ("lfs.txt", model::providers::gh::RemoteEntryKindV1::File),
        ("link", model::providers::gh::RemoteEntryKindV1::Symlink),
    ] {
        let index = named(&original, name);
        let Outcome::Observed(observation) = read(
            &fixture,
            &source,
            Request::File {
                original: original.clone(),
                index,
            },
        )
        .unwrap() else {
            panic!("Expected genuine closed literal content");
        };
        let Page::File(file) = observation.into_page() else {
            panic!("Expected actual hash-verified blob");
        };
        assert_eq!(file.path(), name);
        assert_eq!(file.entry().kind(), kind);
        assert_eq!(
            file.bytes(),
            fx::git(&fixture.repo(), &["cat-file", "blob", file.entry().oid()])
        );
        if name == "binary.bin" {
            assert_eq!(file.bytes(), [0, 1, 2, 255]);
        }
        if name == "link" {
            assert_eq!(file.bytes(), "nested/日本語.txt".as_bytes());
        }
        if name == "lfs.txt" {
            assert!(std::str::from_utf8(file.bytes())
                .unwrap()
                .contains("size 1234567\n"));
        }
    }
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_followup_provider_payload403_requires_real_same_operation_closure() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(default_steps(&objects));
    let mut operation = child_steps(&objects);
    operation[3].status = 1;
    operation[3].stdout.clear();
    operation[3].stderr = b"gh: PRIVATE_DENIED_CAPTURE (HTTP 403)".to_vec();
    steps.extend(operation);
    assert_eq!(steps.len(), 24);
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let root = dir(read(&fixture, &source, Request::DefaultTree).unwrap());
    let index = nested(&root);
    let Outcome::Unavailable(observation) = read(
        &fixture,
        &source,
        Request::Directory {
            original: root,
            index,
        },
    )
    .unwrap() else {
        panic!("Denied payload is not a successful or retained page");
    };
    assert!(matches!(observation.kind(), GithubErrorKind::Permission));
    assert_eq!(observation.identity().account_id, Some(42));
    assert_eq!(observation.repository().id, 410);
    assert_eq!(observation.repository().node_id.as_deref(), Some("R_410"));
    assert!(!format!("{observation:?}").contains("PRIVATE_DENIED_CAPTURE"));
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
#[test]
fn remote_tree_v1_followup_provider_original_sql_epoch_refusal_cannot_adopt_or_fetch_payload() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    assert_eq!(steps.len(), 10);
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let path = fixture.root.join("cache.sqlite3");
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(connection.execute("UPDATE sync_reservations SET generation=2 WHERE host=?1 AND login='user-id:42' AND source=?2 AND generation=1 AND active=1", rusqlite::params!["github.invalid", source.sync_source()]).unwrap(), 1);
    drop(connection);
    let mut phases = Vec::new();
    let error = fx::provider(&fixture)
        .remote_repository_read_guarded_v1(
            source.source().clone(),
            &reference(&source),
            Request::DefaultTree,
            &Default::default(),
            &Default::default(),
            |phase| {
                phases.push(phase);
                model::storage::Cache::open(&path)
                    .unwrap()
                    .validate_repository_catalogue_intent_source(&source)
                    .map_err(|_| GithubError {
                        kind: GithubErrorKind::IdentityChanged,
                        message: "Original local source changed; no payload or rebase".into(),
                    })
            },
        )
        .err()
        .unwrap();
    assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
    assert_eq!(phases.len(), 1);
    assert!(matches!(
        phases[0],
        RemoteRepositoryReadPhaseV1::BeforePayload
    ));
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let generation:u64 = connection.query_row("SELECT generation FROM sync_reservations WHERE host=?1 AND login='user-id:42' AND source=?2", rusqlite::params!["github.invalid", source.sync_source()], |row| row.get(0)).unwrap();
    assert_eq!(
        generation, 2,
        "Independent metadata edit is retained, not adopted/rolled back"
    );
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}

#[test]
fn remote_tree_v1_followup_provider_uid43_cannot_refresh_actual_retained_uid42_blob() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(default_steps(&objects));
    steps.extend(child_steps(&objects));
    steps.extend(file_steps(&objects));
    steps.extend(fx::user(43, "octocat"));
    assert_eq!(steps.len(), 33);
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let root = dir(read(&fixture, &source, Request::DefaultTree).unwrap());
    let child = dir(read(
        &fixture,
        &source,
        Request::Directory {
            index: nested(&root),
            original: root,
        },
    )
    .unwrap());
    let Outcome::Observed(observation) = read(
        &fixture,
        &source,
        Request::File {
            original: child,
            index: 0,
        },
    )
    .unwrap() else {
        panic!("actual private file required before UID loss");
    };
    let Page::File(file) = observation.into_page() else {
        panic!("genuine original blob required");
    };
    assert_eq!(file.bytes(), objects.bytes);
    assert!(std::str::from_utf8(file.bytes())
        .unwrap()
        .contains("old remote saved content"));
    let mut guards = 0;
    let error = fx::provider(&fixture)
        .remote_repository_read_guarded_v1(
            source.source().clone(),
            &reference(&source),
            Request::File {
                original: file.directory().clone(),
                index: file.index(),
            },
            &Default::default(),
            &Default::default(),
            |_| {
                guards += 1;
                Ok(())
            },
        )
        .err()
        .unwrap();
    assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
    assert_eq!(
        guards, 0,
        "different numeric principal cannot reach SQL/payload/final closure"
    );
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}

#[test]
fn remote_tree_v1_followup_provider_final_uid43_refuses_fresh_private_blob_after_original_sql_guards(
) {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = fx::source_steps();
    steps.extend(default_steps(&objects));
    steps.extend(child_steps(&objects));
    steps.extend(file_steps(&objects));
    steps.extend(fx::prefix());
    steps.push(fx::blob_step(&objects));
    steps.extend(fx::suffix(43));
    assert_eq!(steps.len(), 38);
    fixture.scripted_gh(&steps);
    let source = fx::source(&fixture);
    let root = dir(read(&fixture, &source, Request::DefaultTree).unwrap());
    let child = dir(read(
        &fixture,
        &source,
        Request::Directory {
            index: nested(&root),
            original: root,
        },
    )
    .unwrap());
    let Outcome::Observed(observation) = read(
        &fixture,
        &source,
        Request::File {
            original: child,
            index: 0,
        },
    )
    .unwrap() else {
        panic!("actual old private file required");
    };
    let Page::File(file) = observation.into_page() else {
        panic!("actual old private blob required");
    };
    assert_eq!(file.bytes(), objects.bytes);
    let mut phases = Vec::new();
    let error = fx::provider(&fixture)
        .remote_repository_read_guarded_v1(
            source.source().clone(),
            &reference(&source),
            Request::File {
                original: file.directory().clone(),
                index: file.index(),
            },
            &Default::default(),
            &Default::default(),
            |phase| {
                phases.push(phase);
                model::storage::Cache::open(&fixture.root.join("cache.sqlite3"))
                    .unwrap()
                    .validate_repository_catalogue_intent_source(&source)
                    .map_err(|_| GithubError {
                        kind: GithubErrorKind::IdentityChanged,
                        message: "Original source unavailable".into(),
                    })
            },
        )
        .err()
        .unwrap();
    assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
    assert_eq!(phases.len(), 2);
    assert!(matches!(
        phases[0],
        RemoteRepositoryReadPhaseV1::BeforePayload
    ));
    assert!(matches!(
        phases[1],
        RemoteRepositoryReadPhaseV1::BeforeFinalProof
    ));
    // Exact38 proves the same operation actually captured and verified the blob
    // before the closing user43 observation refused presentation, not a fake
    // wrong-user early return or a renewed/higher-limit retry.
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
