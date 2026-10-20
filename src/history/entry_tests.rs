//! TASK-99.3: File history from Search and PR Files reads the local repository only; any
//! other repository explains why, and opening only switches to Repository and asks for one
//! History read. Real repositories only, and nothing is written.
use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::Real,
    workspace::{RepositoryMode, Workspace},
};

fn app(real: &Real) -> App {
    App {
        view: View::Search,
        workspace: Some(
            Workspace::load(&real.root, RepositoryMode::Overview, "HEAD", None)
                .unwrap()
                .unwrap(),
        ),
        local_github_repository_v993: Some("octo/demo".into()),
        ..Default::default()
    }
}

#[test]
fn history_filter_v993_follow_entry_explains_other_repositories_and_missing_sources() {
    let real = Real::new("sha1");
    let before = real.snapshot();
    let mut app = app(&real);
    assert_eq!(same_repository(&app, "OCTO/demo"), Ok(()));
    let other = same_repository(&app, "someone/else").unwrap_err();
    assert!(
        other.contains("needs a local checkout of someone/else")
            && other.contains("this checkout is octo/demo"),
        "{other}"
    );
    app.local_github_repository_v993 = None;
    assert!(same_repository(&app, "octo/demo")
        .unwrap_err()
        .contains("local checkout of octo/demo • this checkout has no GitHub remote"));
    // Without a PR or a search, the keys explain what they need.
    assert!(pr_target(&app).unwrap_err().contains("pull request"));
    assert!(search_target(&app).unwrap_err().contains("Search first"));
    assert!(same_directory(&real.root, &real.root.join(".")));
    // No local repository: nothing opens.
    let mut bare = App::default();
    open(
        &mut bare,
        FileHistoryTargetV993 {
            revision: None,
            path: RawPath(b"README.md".to_vec()),
        },
    );
    assert_eq!(bare.repository_action, None);
    assert!(
        bare.notice.contains("local Git repository"),
        "{}",
        bare.notice
    );
    assert_eq!(real.snapshot(), before);
}

#[test]
fn history_filter_v993_follow_entry_opens_repository_with_one_history_read() {
    let real = Real::new("sha1");
    let before = real.snapshot();
    let mut app = app(&real);
    let target = FileHistoryTargetV993 {
        revision: None,
        path: RawPath(b"docs/guide.md".to_vec()),
    };
    app.dispatch(action(Ok(target.clone())));
    assert_eq!(app.view, View::Repository);
    assert_eq!(
        app.repository_action,
        Some(RowAction::Graph(GraphActionV1::OpenPath {
            revision: None,
            path: target.path.clone(),
        }))
    );
    // A refused target is the same key's explanation.
    let mut app = self::app(&real);
    app.dispatch(action(
        Err("File history reads the local repository".into()),
    ));
    assert_eq!(app.view, View::Search);
    assert_eq!(app.repository_action, None);
    assert_eq!(real.snapshot(), before);
}
