//! Genuine source-factory numeric author observations, never DTO proof builders.
use super::*;

fn replace_parent_users(steps: &mut [GhStep], user: &Value) {
    for step in steps {
        let Ok(mut raw) = serde_json::from_slice::<Value>(&step.stdout) else {
            continue;
        };
        if raw.get("head").is_some() && raw.get("base").is_some() {
            raw["user"] = user.clone();
            step.stdout = raw.to_string().into_bytes();
        }
    }
}

#[test]
fn pending_reviews_v2_raw_parent_retains_numeric_author_independently_of_same_login() {
    for (id, login) in [(51, "current-image-reader"), (42, "different-author-login")] {
        let mut s = inline_setup(|_, steps| {
            replace_parent_users(
                steps,
                &json!({
                    "id": id, "node_id": format!("ACTUAL_USER_{id}"), "login":login, "type":"User",
                }),
            )
        });
        let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let parent = result.proof.observed().parent();
        assert_eq!(parent.author_account_id(), Some(id));
        assert_eq!(parent.summary().pr.author.login, login);
        assert_eq!(parent.database_id(), 1007);
        assert_eq!(parent.repository_id(), 410);
        assert_eq!(parent.head_sha(), s.h.h);
        assert_eq!(result.proof.source_scope().login, "user-id:42");
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_raw_parent_unknown_zero_null_deleted_author_never_infers_numeric_id() {
    for user in [
        json!({"login":"current-image-reader","type":"User"}),
        json!({"id":0,"login":"current-image-reader","type":"User"}),
        json!({"id":null,"node_id":"USER_42","login":"current-image-reader","type":"User"}),
        Value::Null,
    ] {
        let mut s = inline_setup(|_, steps| replace_parent_users(steps, &user));
        let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let parent = result.proof.observed().parent();
        assert_eq!(parent.author_account_id(), None);
        assert_eq!(parent.head_sha(), s.h.h);
        assert_eq!(parent.base_tip_sha(), s.h.b);
        assert_eq!(result.proof.source_scope().login, "user-id:42");
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_raw_parent_author_change_during_actual_image_read_closes_no_proof() {
    let mut s = inline_setup(|_, steps| {
        let parent_index = steps
            .iter()
            .enumerate()
            .filter_map(|(index, step)| {
                serde_json::from_slice::<Value>(&step.stdout)
                    .ok()
                    .filter(|raw| raw.get("head").is_some() && raw.get("base").is_some())
                    .map(|_| index)
            })
            .next_back()
            .unwrap();
        let mut parent: Value = serde_json::from_slice(&steps[parent_index].stdout).unwrap();
        parent["user"]["id"] = json!(42);
        steps[parent_index].stdout = parent.to_string().into_bytes();
    });
    let before_sql = sql_state(&s.fixture);
    let before_git = git_state_resource(&s.fixture);
    let original_author = s.original_files.observed().parent().author_account_id();
    assert_ne!(original_author, Some(42));
    assert!(read(&mut s, &Default::default(), &Default::default()).is_err());
    assert_eq!(sql_state(&s.fixture), before_sql);
    assert_eq!(git_state_resource(&s.fixture), before_git);
    let journal = commands(&s.fixture);
    assert!(journal.iter().all(|row| row["matched"] == true));
    assert!(journal
        .iter()
        .all(|row| row["stdin_hex"].as_str().is_none_or(str::is_empty)));
    assert_eq!(s.workspace.identity.account_id, Some(42));
    assert!(s.workspace.scoped_pr_resource.is_none(), "image input consumed the original selected page; failure cannot silently restore private content");
    assert_eq!(
        s.original_files.observed().parent().author_account_id(),
        original_author
    );
    assert_eq!(s.original_files.observed().parent().head_sha(), s.h.h);
}
