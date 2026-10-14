use super::*;
use crate::{app::View, search::fusion::RankingSource, test_support::Fixture};
use serde_json::json;

#[test]
fn blended_ranked_preferences_exact_positive_account_storage_and_restart() {
    let fixture = Fixture::new();
    let database = fixture.root.join("cache.sqlite3");
    let first = Scope::github("github.invalid", Some(42)).unwrap();
    let reused = Scope::github("github.invalid", Some(43)).unwrap();
    let policy = BlendPolicy {
        enabled: true,
        sources: std::collections::BTreeSet::from([RankingSource::GithubPullRequests]),
        per_source_results: 99,
        per_source_pages: 1,
        ..Default::default()
    };
    let cache = Cache::open(&database).unwrap();
    save_policy(&cache, &first, &policy).unwrap();
    assert_eq!(load_policy(&cache, &first).unwrap(), policy);
    assert_eq!(
        load_policy(&cache, &reused).unwrap(),
        BlendPolicy::default()
    );
    for invalid in [
        Scope::new("github.invalid", "octocat").unwrap(),
        Scope::new("github.invalid", "user-id:0").unwrap(),
        Scope::new("GITHUB.INVALID", "user-id:42").unwrap(),
    ] {
        assert!(save_policy(&cache, &invalid, &policy).is_err());
        assert!(cache.saved_view(&invalid, POLICY_KEY).unwrap().is_none());
    }
    drop(cache);
    assert_eq!(
        load_policy(&Cache::open(&database).unwrap(), &first).unwrap(),
        policy
    );
    assert!(!fixture.root.join("gh-fixture/commands.jsonl").exists());
}

#[test]
fn blended_ranked_preferences_corrupt_version_limits_and_utf8_bytes_refuse_not_default() {
    let fixture = Fixture::new();
    let cache = Cache::open(&fixture.root.join("cache.sqlite3")).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    for value in [
        json!({"version":2,"policy":BlendPolicy::default()}),
        json!({"version":1,"policy":{"enabled":true,"sources":[]}}),
        json!({"version":1,"policy":{"per_source_results":0}}),
    ] {
        cache.save_view(&scope, POLICY_KEY, &value).unwrap();
        assert!(load_policy(&cache, &scope).is_err());
    }
    let mut value = json!({"version":1,"policy":BlendPolicy::default()});
    value["extra"] = json!("💥".repeat(3000));
    let encoded = serde_json::to_string(&value).unwrap();
    assert!(encoded.chars().count() < 8192);
    assert!(encoded.len() > 8192);
    cache.save_view(&scope, POLICY_KEY, &value).unwrap();
    assert!(load_policy(&cache, &scope)
        .unwrap_err()
        .contains("UTF-8 byte budget"));
    assert_eq!(
        cache.saved_view(&scope, POLICY_KEY).unwrap().unwrap(),
        value
    );
    assert!(cache.saved_view_bounded(&scope, POLICY_KEY, 0).is_err());
    assert!(cache
        .saved_view_bounded(&scope, POLICY_KEY, 128 * 1024 + 1)
        .is_err());
    assert!(!fixture.root.join("gh-fixture/commands.jsonl").exists());
}

#[test]
fn blended_ranked_preferences_actions_disable_exact_regex_busy_and_never_submit_query() {
    let mut app = App {
        view: View::Search,
        ..Default::default()
    };
    let mut search = crate::search::workspace::SearchWorkspace::new(
        Scope::new("local", "workspace").unwrap(),
        Vec::new(),
    );
    search.remote_repositories = vec!["example/repo".into()];
    app.search = Some(search);
    for mode in [SearchMode::Exact, SearchMode::Regex] {
        app.search.as_mut().unwrap().mode = mode;
        dispatch(&mut app, BlendAction::Toggle);
        assert!(!app.search.as_ref().unwrap().blend_policy.enabled);
        assert!(app.search_action.is_none());
        assert_eq!(app.overlay, Some(Overlay::Notice));
        app.overlay = None;
    }
    app.search.as_mut().unwrap().mode = SearchMode::Ranked;
    app.loading[View::Search.index()] = true;
    dispatch(&mut app, BlendAction::Toggle);
    assert!(app.search_action.is_none());
    assert!(!app.search.as_ref().unwrap().blend_policy.enabled);
    app.loading[View::Search.index()] = false;
    dispatch(&mut app, BlendAction::Toggle);
    assert_eq!(app.search_action, Some(SearchAction::BlendPreferences));
    assert!(app.search.as_ref().unwrap().blend_policy.enabled);
    assert!(app.search.as_ref().unwrap().page.is_none());
    assert!(app.search.as_ref().unwrap().blend_evidence.is_none());
    open(&mut app);
    assert_eq!(app.overlay, Some(Overlay::SearchBlend));
    dispatch(&mut app, BlendAction::Close);
    assert!(app.overlay.is_none());
}
