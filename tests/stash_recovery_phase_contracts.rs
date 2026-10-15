#[path = "support/fixture_shell_publication_v1.rs"]
mod fixture_shell_publication_v1;
#[path = "support/stash_refresh_failure_v2.rs"]
mod phase;
use std::{collections::BTreeMap, fs};
#[test]
fn stash_recovery_successful_git_then_failed_refresh_retains_actual_witness_not_before_phase() {
    if let Some(root) = std::env::var_os("GHTUI_STASH_PHASE_TEST_ROOT") {
        let root = std::path::PathBuf::from(root);
        let repo = root.join("repo");
        let sha = fs::read_to_string(root.join("selected-sha")).unwrap();
        let workspace = ghtui::workspace::Workspace::load(
            &repo,
            ghtui::workspace::RepositoryMode::Overview,
            "HEAD",
            None,
        )
        .unwrap()
        .unwrap();
        let origin = workspace.actor_origin.clone().unwrap();
        let context = ghtui::local_actions::LocalContext::from_workspace(&workspace, None, None);
        let plan = ghtui::local_actions::LocalPlan::prepare(
            ghtui::local_actions::LocalCommand::StashPop,
            &context,
            &BTreeMap::from([("id".into(), sha.clone())]),
            Default::default(),
        )
        .unwrap();
        let error = plan
            .execute_stash_with_inspection_v1(true, Default::default(), origin.clone())
            .unwrap_err();
        phase::assert_success_and_failed_refresh(&root);
        assert!(error.failed_intent().is_some(),"Successful Git + failed refresh must retain its genuine pre-execution source/stash witness");
        assert!(
            error.message.contains("succeeded"),
            "Cannot label successful Git command as before execution: {}",
            error.message
        );
        let witness = error.failed_intent().unwrap();
        assert_eq!(witness.sha(), sha);
        assert!(std::sync::Arc::ptr_eq(witness.origin(), &origin));
        assert!(
            witness.command_succeeded(),
            "Typed original phase must retain the actual successful Git result"
        );
    } else {
        let directory = tempfile::tempdir().unwrap();
        let (repo, sha) = phase::setup(directory.path());
        fs::write(directory.path().join("selected-sha"), &sha).unwrap();
        phase::install(directory.path());
        let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","stash_recovery_successful_git_then_failed_refresh_retains_actual_witness_not_before_phase","--nocapture"]).env("GHTUI_STASH_PHASE_TEST_ROOT",directory.path()).env("PATH",phase::child_path(directory.path())).output().unwrap();
        assert!(
            output.status.success(),
            "Actual isolated component process:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(repo.join("file")).unwrap(),
            b"successful stash content\n"
        );
        assert!(phase::git(&repo, &["stash", "list"]).is_empty());
        phase::assert_success_and_failed_refresh(directory.path());
    }
}

#[test]
fn stash_recovery_successful_pop_held_refresh_recreated_locator_refuses_new_workspace() {
    const NAME: &str =
        "stash_recovery_successful_pop_held_refresh_recreated_locator_refuses_new_workspace";
    if let Some(root) = std::env::var_os("GHTUI_STASH_PHASE_RECREATED_ROOT") {
        let root = std::path::PathBuf::from(root);
        let workspace = ghtui::workspace::Workspace::load(
            &root.join("repo"),
            ghtui::workspace::RepositoryMode::Overview,
            "HEAD",
            None,
        )
        .unwrap()
        .unwrap();
        let origin = workspace.actor_origin.clone().unwrap();
        let sha = fs::read_to_string(root.join("selected-sha")).unwrap();
        let plan = ghtui::local_actions::LocalPlan::prepare(
            ghtui::local_actions::LocalCommand::StashPop,
            &ghtui::local_actions::LocalContext::from_workspace(&workspace, None, None),
            &BTreeMap::from([("id".into(), sha.clone())]),
            Default::default(),
        )
        .unwrap();
        let error = plan.execute_stash_with_inspection_v1(true,Default::default(),origin.clone()).expect_err("A different live repository at the original locator cannot become the successful original-write result");
        let witness = error
            .failed_intent()
            .expect("Actual successful write retains original witness even after source loss");
        assert!(witness.command_succeeded());
        assert_eq!(witness.sha(), sha);
        assert!(std::sync::Arc::ptr_eq(witness.origin(), &origin));
        assert!(
            error.message.contains("succeeded") && error.message.contains("Original"),
            "{}",
            error.message
        );
    } else {
        let directory = tempfile::tempdir().unwrap();
        let (repo, sha) = phase::setup(directory.path());
        fs::write(directory.path().join("selected-sha"), sha).unwrap();
        phase::replacement(directory.path());
        let replacement_head = phase::git(
            &directory.path().join("replacement"),
            &["rev-parse", "HEAD"],
        );
        phase::install_held_reload(directory.path());
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env("GHTUI_STASH_PHASE_RECREATED_ROOT", directory.path())
            .env("PATH", phase::child_path(directory.path()))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let original = phase::replace_held_locator(directory.path(), &repo);
        let status = child.wait().unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read(original.join("file")).unwrap(),
            b"successful stash content\n"
        );
        assert!(phase::git(&original, &["stash", "list"]).is_empty());
        assert_eq!(
            fs::read(repo.join("file")).unwrap(),
            b"replacement private body\n"
        );
        assert_eq!(phase::git(&repo, &["rev-parse", "HEAD"]), replacement_head);
        let records = phase::records(directory.path());
        assert_eq!(
            records
                .iter()
                .filter(|r| r
                    .windows(2)
                    .any(|w| w == [b"stash".to_vec(), b"pop".to_vec()]))
                .count(),
            1
        );
        assert!(!records.iter().any(|r| r
            .windows(2)
            .any(|w| w == [b"stash".to_vec(), b"drop".to_vec()])));
    }
}
