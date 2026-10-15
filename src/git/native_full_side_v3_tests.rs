use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::{git, text, Real},
    workspace::{RepositoryMode, Workspace},
};
use std::{fs, os::unix::fs::symlink};

fn source(real: &Real, mode: RepositoryMode) -> Arc<MutableDiffSourceV2> {
    let loaded = Workspace::load(&real.root, mode, "HEAD", None)
        .unwrap()
        .unwrap();
    let intent = MutableDiffIntentV2::capture(&loaded).unwrap();
    let operation = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    let source = MutableDiffSourceV2::read(intent, &operation).unwrap();
    operation.seal().unwrap();
    source
}
fn operation(source: &MutableDiffSourceV2) -> LocalGitNavigationReadV1 {
    LocalGitNavigationReadV1::begin(source.intent().origin(), Default::default()).unwrap()
}
#[test]
fn native_full_side_provider_sha1_actual_six_sides_preserve_git() {
    six_sides("sha1");
}
#[test]
fn native_full_side_provider_sha256_actual_six_sides_preserve_git() {
    six_sides("sha256");
}
fn six_sides(format: &str) {
    let real = Real::new(format);
    let before = real.snapshot();
    for (mode, old, new) in [
        (
            RepositoryMode::Working,
            b"staged bytes\n".as_slice(),
            b"dirty bytes\n".as_slice(),
        ),
        (
            RepositoryMode::Staged,
            b"saved original bytes\n",
            b"staged bytes\n",
        ),
        (
            RepositoryMode::Uncommitted,
            b"saved original bytes\n",
            b"dirty bytes\n",
        ),
    ] {
        let source = source(&real, mode);
        for (side, expected) in [(DiffSide::Old, old), (DiffSide::New, new)] {
            real.clear_trace();
            let op = operation(&source);
            let material = MutableFullSideV3::read(source.clone(), 0, side, &op).unwrap();
            assert_eq!(material.content(), expected);
            assert_eq!(material.kind(), &BlobPreviewKind::Text);
            assert!(!material.absent());
            assert_eq!(
                material.raw_worktree(),
                side == DiffSide::New && mode != RepositoryMode::Staged
            );
            if material.raw_worktree() {
                assert!(material.object().is_none());
                assert!(material.label().contains("RAW UNSAVED"));
            } else {
                assert!(original_actor_saved_sha(material.object().unwrap()));
            }
            let actual = real.children().len();
            assert!(actual > 0);
            assert_eq!(op.seal().unwrap().children_started(), actual);
            assert_eq!(real.snapshot(), before);
        }
    }
}
#[test]
fn native_full_side_provider_saved_index_bytes_not_dirty_fallback() {
    let real = Real::new("sha256");
    let source = source(&real, RepositoryMode::Working);
    let op = operation(&source);
    let material = MutableFullSideV3::read(source, 0, DiffSide::Old, &op).unwrap();
    assert_eq!(
        material.object().unwrap(),
        text(&real.root, &["rev-parse", ":tracked"])
    );
    assert_eq!(
        material.content(),
        git(&real.root, &["show", ":tracked"], None)
    );
    assert!(material.label().contains("INDEX stage0"));
    op.seal().unwrap();
}
#[test]
fn native_full_side_provider_current_raw_bytes_revalidated_not_just_metadata() {
    let real = Real::new("sha1");
    let source = source(&real, RepositoryMode::Working);
    let first = operation(&source);
    let material = MutableFullSideV3::read(source.clone(), 0, DiffSide::New, &first).unwrap();
    first.seal().unwrap();
    fs::write(real.root.join("tracked"), b"other bytes\n").unwrap();
    let next = operation(&source);
    assert!(material
        .validate_current(&next)
        .unwrap_err()
        .contains("stale"));
    assert_eq!(
        fs::read(real.root.join("tracked")).unwrap(),
        b"other bytes\n"
    );
}
#[test]
fn native_full_side_provider_actual_wrong_root_fd_refused_before_member_open() {
    let real = Real::new("sha1");
    let source = source(&real, RepositoryMode::Working);
    let op = operation(&source);
    let retained = real.directory.path().join("original-repo-retained");
    fs::rename(&real.root, &retained).unwrap();
    fs::create_dir(&real.root).unwrap();
    // A real wrong directory at the exact captured locator, with a dangling
    // member symlink: rejection MUST be the original-root FD check, not a later
    // path/content/read fallback. Original repo remains recoverable intact.
    symlink(
        real.directory.path().join("never-follow"),
        real.root.join("tracked"),
    )
    .unwrap();
    let error = read_worktree(
        source.intent().origin().source(),
        &RawPath(b"tracked".to_vec()),
        &op,
    )
    .unwrap_err();
    assert!(
        error.contains(
            "Opened root descriptor is not the captured original repository; no body read"
        ),
        "{error}"
    );
    assert_eq!(
        fs::read(retained.join("tracked")).unwrap(),
        b"dirty bytes\n"
    );
    let wrong = File::open(&real.root).unwrap();
    assert!(verify_root(
        &wrong,
        source.intent().origin().source().root_file_identity_v3()
    )
    .is_err());
}
#[test]
fn native_full_side_provider_parent_symlink_cannot_escape_original_root() {
    let real = Real::new("sha1");
    let source = source(&real, RepositoryMode::Working);
    let op = operation(&source);
    let outside = real.directory.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret"), b"NO OUTSIDE CONTENT").unwrap();
    symlink(&outside, real.root.join("escape")).unwrap();
    let error = read_worktree(
        source.intent().origin().source(),
        &RawPath(b"escape/secret".to_vec()),
        &op,
    )
    .unwrap_err();
    assert!(error.contains("No-follow"));
    assert_eq!(
        fs::read(outside.join("secret")).unwrap(),
        b"NO OUTSIDE CONTENT"
    );
}
#[test]
fn native_full_side_provider_symlink_literal_never_follows_target() {
    let real = Real::new("sha256");
    fs::remove_file(real.root.join("tracked")).unwrap();
    symlink("old-index-literal", real.root.join("tracked")).unwrap();
    git(&real.root, &["add", "--", "tracked"], None);
    fs::remove_file(real.root.join("tracked")).unwrap();
    let outside = real.directory.path().join("outside-sensitive");
    fs::write(&outside, b"OUTSIDE SECRET MUST NEVER LOAD").unwrap();
    symlink(&outside, real.root.join("tracked")).unwrap();
    let source = source(&real, RepositoryMode::Working);
    let op = operation(&source);
    let material = MutableFullSideV3::read(source, 0, DiffSide::New, &op).unwrap();
    assert_eq!(material.kind(), &BlobPreviewKind::Symlink);
    assert_eq!(material.content(), outside.as_os_str().as_bytes());
    assert!(!material
        .content()
        .windows(14)
        .any(|bytes| bytes == b"OUTSIDE SECRET"));
    op.seal().unwrap();
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"OUTSIDE SECRET MUST NEVER LOAD"
    );
}
#[test]
fn native_full_side_provider_special_fifo_refused_before_open() {
    let real = Real::new("sha1");
    let source = source(&real, RepositoryMode::Working);
    let op = operation(&source);
    fs::remove_file(real.root.join("tracked")).unwrap();
    let name = cstring(real.root.join("tracked").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(read_worktree(
        source.intent().origin().source(),
        &RawPath(b"tracked".to_vec()),
        &op
    )
    .unwrap_err()
    .contains("special file refused before open"));
}
#[test]
fn native_full_side_provider_oversize_raw_refuses_before_body() {
    let real = Real::new("sha1");
    let source = source(&real, RepositoryMode::Working);
    let op = operation(&source);
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(real.root.join("tracked"))
        .unwrap();
    file.set_len(SIDE_CAP as u64 + 1).unwrap();
    assert!(read_worktree(
        source.intent().origin().source(),
        &RawPath(b"tracked".to_vec()),
        &op
    )
    .unwrap_err()
    .contains("body not read"));
}
#[test]
fn native_full_side_provider_raw_crlf_is_not_normalized_patch_content() {
    let real = Real::new("sha1");
    git(&real.root, &["config", "core.autocrlf", "true"], None);
    fs::write(real.root.join("tracked"), b"RAW\r\nunsaved\r\n").unwrap();
    let source = source(&real, RepositoryMode::Working);
    let op = operation(&source);
    let material = MutableFullSideV3::read(source.clone(), 0, DiffSide::New, &op).unwrap();
    assert_eq!(material.content(), b"RAW\r\nunsaved\r\n");
    assert!(material.label().contains("not Git-normalized"));
    assert!(!source
        .result()
        .patch
        .windows(5)
        .any(|value| value == b"RAW\r\n"));
    op.seal().unwrap();
}
#[test]
fn native_full_side_provider_actual_replacement_and_graft_refuse_no_substitution() {
    for graft in [false, true] {
        let real = Real::new("sha1");
        if graft {
            fs::create_dir_all(real.root.join(".git/info")).unwrap();
            fs::write(
                real.root.join(".git/info/grafts"),
                format!("{}\n", real.head),
            )
            .unwrap();
        } else {
            git(&real.root, &["replace", &real.head, &real.original], None);
        }
        let source = source(&real, RepositoryMode::Working);
        let op = operation(&source);
        let error = MutableFullSideV3::read(source, 0, DiffSide::New, &op).unwrap_err();
        assert!(error.contains("replacement/graft"), "{error}");
        assert_eq!(
            fs::read(real.root.join("tracked")).unwrap(),
            b"dirty bytes\n"
        );
    }
}
#[test]
fn native_full_side_provider_sealed_or_cancelled_original_operation_cannot_read() {
    let real = Real::new("sha1");
    let source = source(&real, RepositoryMode::Working);
    real.clear_trace();
    let sealed = operation(&source);
    sealed.seal().unwrap();
    assert!(MutableFullSideV3::read(source.clone(), 0, DiffSide::New, &sealed).is_err());
    assert!(real.children().is_empty());
    let cancelled = operation(&source);
    cancelled.cancellation().cancel();
    assert!(MutableFullSideV3::read(source, 0, DiffSide::Old, &cancelled).is_err());
    assert!(real.children().is_empty());
}
#[test]
fn native_full_side_provider_genuine_working_gitlink_does_not_infer_nested_head_from_index() {
    let real = Real::new("sha1");
    let nested = real.root.join("module");
    fs::create_dir(&nested).unwrap();
    git(&nested, &["init", "-q", "--initial-branch=main"], None);
    git(&nested, &["config", "user.name", "Nested Recorder"], None);
    git(
        &nested,
        &["config", "user.email", "nested@example.invalid"],
        None,
    );
    git(&nested, &["config", "commit.gpgsign", "false"], None);
    fs::write(nested.join("inside"), b"nested old bytes\n").unwrap();
    git(&nested, &["add", "--", "inside"], None);
    git(&nested, &["commit", "-qm", "Nested old"], None);
    let old = text(&nested, &["rev-parse", "HEAD"]);
    git(
        &real.root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{old},module"),
        ],
        None,
    );
    fs::write(nested.join("inside"), b"nested new bytes\n").unwrap();
    git(&nested, &["add", "--", "inside"], None);
    git(&nested, &["commit", "-qm", "Nested new"], None);
    let new = text(&nested, &["rev-parse", "HEAD"]);
    assert_ne!(old, new);
    let source = source(&real, RepositoryMode::Working);
    let index = source
        .members()
        .iter()
        .position(|member| {
            member
                .path(DiffSide::New)
                .is_some_and(|path| path.0 == b"module")
        })
        .expect("Genuine changed gitlink source member");
    let op = operation(&source);
    let material = MutableFullSideV3::read(source, index, DiffSide::New, &op).unwrap();
    assert_eq!(material.kind(), &BlobPreviewKind::Submodule);
    assert!(material.raw_worktree());
    assert!(material.content().is_empty());
    assert_eq!(material.object(), Some(old.as_str()));
    assert_ne!(material.object(), Some(new.as_str()));
    assert!(material.label().contains("NOT the nested checked-out HEAD"));
    assert!(material.label().contains("bytes were NOT read"));
    op.seal().unwrap();
    assert_eq!(
        fs::read(nested.join("inside")).unwrap(),
        b"nested new bytes\n"
    );
}
#[test]
fn native_full_side_provider_actual_detached_parent_replacement_same_git_fingerprint_refused() {
    let real = Real::new("sha1");
    let parent = real.root.join("nested");
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("leaf"), b"staged nested bytes\n").unwrap();
    git(&real.root, &["add", "--", "nested/leaf"], None);
    fs::write(parent.join("leaf"), b"unsaved nested content\n").unwrap();
    let source = source(&real, RepositoryMode::Working);
    let before = source.intent().observed();
    let old = fs::metadata(&parent).unwrap().ino();
    let retained = real.directory.path().join("detached-parent-retained");
    let op = operation(&source);
    let result = read_worktree_with_recheck_v3(
        source.intent().origin().source(),
        &RawPath(b"nested/leaf".to_vec()),
        &op,
        || {
            fs::rename(&parent, &retained).unwrap();
            fs::create_dir(&parent).unwrap();
            fs::write(parent.join("leaf"), b"unsaved nested content\n").unwrap();
        },
    );
    assert_ne!(old, fs::metadata(&parent).unwrap().ino());
    assert_eq!(
        fs::read(retained.join("leaf")).unwrap(),
        fs::read(parent.join("leaf")).unwrap()
    );
    assert_eq!(real.repository.fingerprint().unwrap(),before,"Installed Git's exact fingerprint alone cannot distinguish these genuine identical raw-path replacements");
    source.validate_current(&op).unwrap();
    assert!(result.is_err(),"An actual detached parent MUST refuse even though current replacement has identical bytes and Git fingerprint");
    assert!(result.unwrap_err().contains("parent chain"));
    op.seal().unwrap();
}
#[test]
fn native_full_side_provider_actual_growing_index_blob_buffer_charges_capacity_and_labels() {
    let real = Real::new("sha1");
    let content = vec![b'x'; 1024 * 1024 + 1];
    fs::write(real.root.join("tracked"), &content).unwrap();
    git(&real.root, &["add", "--", "tracked"], None);
    let source = source(&real, RepositoryMode::Staged);
    let op = operation(&source);
    let material = MutableFullSideV3::read(source, 0, DiffSide::New, &op).unwrap();
    assert_eq!(material.content, content);
    assert!(material.content.capacity()>material.content.len(),"Genuine pipe capture currently retains spare capacity; test that exact observed allocation");
    let actual = material.content.capacity()
        + material.label.capacity()
        + material.object.as_ref().map_or(0, String::capacity)
        + std::mem::size_of::<MutableFullSideV3>();
    assert!(
        material.retained_bytes() >= actual,
        "Actual retained Vec/String allocation, not just blob byte length, must be charged"
    );
    op.seal().unwrap();
}
