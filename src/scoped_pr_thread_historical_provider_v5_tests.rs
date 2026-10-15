//! DECLARED / UNRUN. Real Git object bytes plus genuine original comment
//! producers. No test manufactures a historical read or current placement.
use super::*;
use crate::immutable_git_v1_test_support as history;
use crate::immutable_git_v1_test_support as git_history;
#[path = "../tests/support/thread_historical_git_v5.rs"]
mod actual_git;
use crate::providers::gh::{
    HistoricalCommitRoleV5 as Role, HistoricalContentKindV5 as Kind, PrImmutableFileLimitsV1,
};

fn observed(h: &history::GitImagesV1) -> (Value, Value, Value) {
    let mut thread = selected_node(false, true);
    let row = &mut thread["comments"]["nodes"][0];
    row["path"] = json!(history::PATH);
    row["commit"]["oid"] = json!(h.h);
    row["originalCommit"]["oid"] = json!(h.m);
    let mut dedicated = selected_comment();
    dedicated["path"] = json!(history::PATH);
    dedicated["commit"]["oid"] = json!(h.h);
    dedicated["originalCommit"]["oid"] = json!(h.m);
    let mut rest = current_rest();
    rest["path"] = json!(history::PATH);
    rest["commit_id"] = json!(h.h);
    rest["original_commit_id"] = json!(h.m);
    (thread, dedicated, rest)
}
pub(super) fn opening(h: &history::GitImagesV1) -> Vec<GhStep> {
    let (thread, dedicated, rest) = observed(h);
    let mut steps = opening_steps(thread);
    steps.extend(observation(dedicated, rest));
    steps
}
pub(super) fn objects(h: &history::GitImagesV1, role: Role, blob: bool) -> Vec<GhStep> {
    let (commit, root, trees, sha, bytes) = if role == Role::Associated {
        (&h.h, &h.th, &h.new_trees, &h.new_blob, &h.new_bytes)
    } else {
        (&h.m, &h.tm, &h.old_trees, &h.old_blob, &h.old_bytes)
    };
    let mut steps = vec![get(
        &format!("repos/{REPO}/git/commits/{commit}"),
        json!({"sha":commit,"tree":{"sha":root}}),
    )];
    for (sha, raw) in trees {
        steps.push(get(&format!("repos/{REPO}/git/trees/{sha}"), raw.clone()));
    }
    if blob {
        steps.push(GhStep::read(
            &[
                "api",
                &format!("repos/{REPO}/git/blobs/{sha}"),
                "--hostname",
                HOST,
                "--method",
                "GET",
                "--header",
                "Accept: application/vnd.github.raw+json",
                "--allow-escape-sequences",
            ],
            std::str::from_utf8(bytes).unwrap(),
        ));
    }
    steps
}
fn count(real: &Real) -> usize {
    fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .count()
}

#[test]
fn review_threads_v5_historical_genuine_associated_and_original_content_is_not_current_placement() {
    for role in [Role::Associated, Role::Original] {
        let dir = tempfile::tempdir().unwrap();
        let h = history::GitImagesV1::create(dir.path(), false);
        let mut steps = opening(&h);
        steps.extend(objects(&h, role, true));
        steps.extend(close_steps());
        let real = real(steps);
        with_original_limits(&real, 24, |a| {
            let selected = Arc::new(observe(&a).unwrap());
            let result = a
                .measured
                .pr_thread_historical_context_v5(
                    selected.clone(),
                    role,
                    &Default::default(),
                    a.cancel,
                    a.deadline,
                )
                .unwrap();
            assert!(Arc::ptr_eq(result.selected(), &selected));
            assert_eq!(result.role(), role);
            assert_eq!(result.path(), history::PATH.as_bytes());
            assert_eq!(result.kind(), Kind::Text);
            assert_eq!(
                result.commit(),
                if role == Role::Associated { &h.h } else { &h.m }
            );
            assert_ne!(result.commit(), selected.original().parent().head_sha());
            assert_eq!(
                result.root_tree(),
                if role == Role::Associated {
                    &h.th
                } else {
                    &h.tm
                }
            );
            assert_eq!(
                result.bytes(),
                Some(if role == Role::Associated {
                    h.new_bytes.as_slice()
                } else {
                    h.old_bytes.as_slice()
                })
            );
            assert_eq!(result.tree_reads(), 2);
            assert!(result.tree_rows() >= 2);
            assert!(result.retained_image_bytes_v5().unwrap() < 4 * 1024 * 1024);
            (*a.close)().unwrap();
            let before = count(&real);
            assert!(a
                .measured
                .pr_thread_historical_context_v5(
                    selected,
                    role,
                    &Default::default(),
                    a.cancel,
                    a.deadline
                )
                .is_err());
            assert_eq!(
                count(&real),
                before,
                "sealed original pool launches no further child"
            );
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        exact_protocol(&real, 0);
    }
}
#[test]
fn review_threads_v5_historical_renamed_observed_candidate_absent_does_not_guess_old_path_or_line()
{
    let dir = tempfile::tempdir().unwrap();
    let old = (0..24)
        .map(|line| format!("Historical unchanged context {line:03}\n"))
        .collect::<String>();
    let new = old.replace("context 002", "changed 002");
    let h = history::GitImagesV1::create_renamed_with_bytes(
        dir.path(),
        false,
        old.as_bytes(),
        new.as_bytes(),
    );
    let mut steps = opening(&h);
    steps.extend(objects(&h, Role::Original, false));
    steps.extend(close_steps());
    let real = real(steps);
    with_original_limits(&real, 24, |a| {
        let selected = Arc::new(observe(&a).unwrap());
        let result = a
            .measured
            .pr_thread_historical_context_v5(
                selected,
                Role::Original,
                &Default::default(),
                a.cancel,
                a.deadline,
            )
            .unwrap();
        assert_eq!(result.kind(), Kind::CandidatePathAbsent);
        assert_eq!(result.path(), history::PATH.as_bytes());
        assert!(result.object_sha().is_none());
        assert!(result.bytes().is_none());
        assert_eq!(result.commit(), h.m);
        assert_eq!(result.tree_reads(), 2);
        // Even an exact complete-tree absence must finish the original source
        // bracket before any user-visible result; no old-path substitution.
        (*a.close)().unwrap();
    });
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v5_historical_original_producer_runner_token_clock_and_limits_refuse_zero_children(
) {
    for variation in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        let h = history::GitImagesV1::create(dir.path(), false);
        let mut steps = opening(&h);
        steps.extend(close_steps());
        let real = real(steps);
        with_original_limits(&real, 24, |a| {
            let selected = Arc::new(observe(&a).unwrap());
            let mut provider = a.measured.clone();
            let other = CancellationToken::default();
            let cancel = if variation == 1 { &other } else { a.cancel };
            let deadline = if variation == 2 {
                Instant::now().checked_sub(Duration::from_secs(1)).unwrap()
            } else {
                a.deadline
            };
            if variation == 0 {
                provider = provider.with_runner(crate::providers::runner::CommandRunner::default());
            }
            let mut limits = PrImmutableFileLimitsV1::default();
            if variation == 3 {
                limits.max_pool_bytes *= 2;
            }
            if variation == 4 {
                limits.max_path_depth = 1;
            }
            let before = count(&real);
            assert!(provider
                .pr_thread_historical_context_v5(
                    selected,
                    Role::Associated,
                    &limits,
                    cancel,
                    deadline
                )
                .is_err());
            assert_eq!(
                count(&real),
                before,
                "foreign/reopened/invalid historical phase must start no child"
            );
            (*a.close)().unwrap();
        });
        exact_protocol(&real, 0);
    }
}
#[test]
fn review_threads_v5_historical_wrong_commit_truncated_or_corrupt_tree_and_blob_never_admit_content(
) {
    for variation in 0..4 {
        let dir = tempfile::tempdir().unwrap();
        let h = history::GitImagesV1::create(dir.path(), false);
        let mut rows = objects(&h, Role::Associated, true);
        if variation == 0 {
            rows[0] = get(
                &format!("repos/{REPO}/git/commits/{}", h.h),
                json!({"sha":"f".repeat(40),"tree":{"sha":h.th}}),
            );
            rows.truncate(1);
        } else if variation == 1 || variation == 2 {
            let mut raw = h.new_trees[0].1.clone();
            if variation == 1 {
                raw["truncated"] = json!(true);
            } else {
                raw["tree"][0]["path"] = json!("different-raw-name");
            }
            rows[1] = get(&format!("repos/{REPO}/git/trees/{}", h.th), raw);
            rows.truncate(2);
        } else {
            rows.last_mut().unwrap().stdout = b"wrong blob bytes".to_vec();
        }
        let mut steps = opening(&h);
        steps.extend(rows);
        let real = real(steps);
        with_original_limits(&real, 24, |a| {
            let selected = Arc::new(observe(&a).unwrap());
            assert!(a
                .measured
                .pr_thread_historical_context_v5(
                    selected,
                    Role::Associated,
                    &Default::default(),
                    a.cancel,
                    a.deadline
                )
                .is_err());
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        exact_protocol(&real, 0);
    }
}
#[test]
fn review_threads_v5_historical_same_issuer_less_restricted_clone_only_narrows_original_clock_child_capture_and_timeout(
) {
    let dir = tempfile::tempdir().unwrap();
    let h = history::GitImagesV1::create(dir.path(), false);
    let mut steps = opening(&h);
    steps.extend(close_steps());
    let real = real(steps);
    with_original_limits(&real, 24, |a| {
        let issuer = a
            .measured
            .clone()
            .with_timeout(Duration::from_secs(3))
            .with_scoped_read_window(a.deadline, 128 * 1024)
            .unwrap();
        let selected = issuer
            .original_thread_comment_v2(
                a.identity,
                a.actual.clone(),
                "COMMENT_ACTUAL_9",
                9009,
                a.cancel,
                a.deadline,
            )
            .unwrap();
        let original = selected
            .historical_read_phase_v5(&issuer, a.cancel, a.deadline)
            .unwrap();
        let wider = a
            .measured
            .clone()
            .with_timeout(Duration::from_secs(120))
            .with_scoped_read_window(a.deadline + Duration::from_secs(60), 1024 * 1024)
            .unwrap();
        let before = count(&real);
        let retained = selected
            .historical_read_phase_v5(&wider, a.cancel, a.deadline + Duration::from_secs(60))
            .unwrap();
        assert_eq!(retained,original,"Same runner/pool clone cannot renew original queued window, capture or per-child timeout");
        let earlier = Instant::now() + Duration::from_secs(2);
        let narrow = a
            .measured
            .clone()
            .with_timeout(Duration::from_millis(10))
            .with_scoped_read_window(earlier, 1024)
            .unwrap();
        let restricted = selected
            .historical_read_phase_v5(&narrow, a.cancel, a.deadline)
            .unwrap();
        assert_eq!(restricted, (earlier, 1024, Duration::from_millis(10)));
        assert_eq!(count(&real), before);
        (*a.close)().unwrap();
    });
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v5_historical_actual_symlink_and_submodule_git_entries_are_never_followed() {
    for symlink in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut h = history::GitImagesV1::create(dir.path(), false);
        actual_git::special(dir.path(), &mut h, symlink);
        let mut steps = opening(&h);
        steps.extend(objects(&h, Role::Associated, symlink));
        steps.extend(close_steps());
        let real = real(steps);
        with_original_limits(&real, 24, |a| {
            let selected = Arc::new(observe(&a).unwrap());
            let result = a
                .measured
                .pr_thread_historical_context_v5(
                    selected,
                    Role::Associated,
                    &Default::default(),
                    a.cancel,
                    a.deadline,
                )
                .unwrap();
            assert_eq!(
                result.kind(),
                if symlink {
                    Kind::Symlink
                } else {
                    Kind::Submodule
                }
            );
            assert_eq!(
                result.mode(),
                Some(if symlink { "120000" } else { "160000" })
            );
            assert_eq!(result.object_sha(), Some(h.new_blob.as_str()));
            assert_eq!(result.bytes(), symlink.then_some(h.new_bytes.as_slice()));
            (*a.close)().unwrap();
        });
        exact_protocol(&real, 0);
    }
}
#[test]
fn review_threads_v5_historical_binary_lfs_and_oversized_are_bounded_special_content_not_followed()
{
    for variation in 0..3 {
        let dir = tempfile::tempdir().unwrap();
        let bytes: &[u8] = if variation == 0 {
            b"binary\0bytes\n"
        } else {
            b"version https://git-lfs.github.com/spec/v1\noid sha256:abcdef\nsize 9999\n"
        };
        let h = history::GitImagesV1::create_with_bytes(dir.path(), false, history::OLD, bytes);
        let mut object_steps = objects(&h, Role::Associated, variation != 2);
        if variation == 0 {
            object_steps.last_mut().unwrap().stdout = bytes.to_vec();
        }
        let mut steps = opening(&h);
        steps.extend(object_steps);
        steps.extend(close_steps());
        let real = real(steps);
        with_original_limits(&real, 24, |a| {
            let selected = Arc::new(observe(&a).unwrap());
            let mut limits = PrImmutableFileLimitsV1::default();
            if variation == 2 {
                limits.max_blob_bytes = 1;
            }
            let result = a
                .measured
                .pr_thread_historical_context_v5(
                    selected,
                    Role::Associated,
                    &limits,
                    a.cancel,
                    a.deadline,
                )
                .unwrap();
            assert_eq!(
                result.kind(),
                match variation {
                    0 => Kind::Binary,
                    1 => Kind::LfsPointer,
                    _ => Kind::Oversized,
                }
            );
            assert_eq!(result.bytes().is_none(), variation == 2);
            (*a.close)().unwrap();
            assert_eq!(a.captures.consumed_requests(), 0);
        });
        exact_protocol(&real, 0);
    }
}
