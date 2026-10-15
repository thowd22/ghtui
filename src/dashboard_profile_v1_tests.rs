use super::*;
use std::os::unix::fs::PermissionsExt;
fn private_temp() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn profile() -> SavedDashboardV1 {
    SavedDashboardV1 {
        kind: "ghtui-dashboard".into(),
        version: 1,
        host: "github.invalid".into(),
        account_id: 42,
        repositories: vec!["example/a".into(), "example/b".into()],
        window_days: 7,
        layout: Default::default(),
    }
}
#[test]
fn dashboards_layout_reorders_hides_and_refuses_last_visible_or_unknown_widget() {
    let mut layout = DashboardLayoutV1::default();
    assert_eq!(layout.move_slot(0, true).unwrap(), 1);
    assert_eq!(layout.shown().next(), Some(MetricKind::NeedsMyReview));
    layout.toggle(1).unwrap();
    assert!(!layout.shown().any(|kind| kind == MetricKind::OpenPrs));
    for index in 1..8 {
        if layout.slots[index].visible {
            layout.toggle(index).unwrap();
        }
    }
    let before = layout.clone();
    assert!(layout.toggle(0).is_err());
    assert_eq!(layout, before);
    assert!(layout.move_slot(8, false).is_err());
    assert_eq!(layout.move_slot(0, false).unwrap(), 0);
    assert_eq!(layout.move_slot(7, true).unwrap(), 7);
}
#[test]
fn dashboards_profile_rejects_foreign_account_scope_expansion_versions_and_bad_layout() {
    let original = profile();
    let scope = original.scope().unwrap();
    original
        .validate_for(&scope, &original.repositories)
        .unwrap();
    assert!(original
        .validate_for(
            &Scope::github("github.invalid", Some(43)).unwrap(),
            &original.repositories
        )
        .is_err());
    assert!(original
        .validate_for(&scope, &["example/a".into()])
        .is_err());
    for mode in 0..7 {
        let mut bad = original.clone();
        match mode {
            0 => bad.version = 2,
            1 => bad.repositories.push("example/a".into()),
            2 => bad.repositories = vec!["Example/A".into()],
            3 => bad.window_days = 0,
            4 => bad.window_days = 366,
            5 => bad.layout.slots[1].kind = bad.layout.slots[0].kind,
            _ => bad
                .layout
                .slots
                .iter_mut()
                .for_each(|slot| slot.visible = false),
        }
        assert!(bad.validate_for(&scope, &original.repositories).is_err());
    }
    let mut value = serde_json::to_value(original).unwrap();
    value["cached_metrics"] = serde_json::json!({"private":"not preferences"});
    assert!(serde_json::from_value::<SavedDashboardV1>(value).is_err());
}
#[test]
fn dashboards_sql_default_profile_roundtrip_account_isolation_preserves_drafts_and_other_views() {
    let temp = private_temp();
    let path = temp.path().join("cache.sqlite3");
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let cache = Cache::open(&path).unwrap();
    cache
        .put_draft(&scope, "review", "original pending body")
        .unwrap();
    cache
        .save_view(&scope, "unrelated", &serde_json::json!({"keep":true}))
        .unwrap();
    let mut original = profile();
    original.layout.move_slot(7, false).unwrap();
    original.layout.toggle(0).unwrap();
    original.save(&cache).unwrap();
    drop(cache);
    let cache = Cache::open(&path).unwrap();
    assert_eq!(
        SavedDashboardV1::load(&cache, &scope, &original.repositories).unwrap(),
        original
    );
    assert!(SavedDashboardV1::load(
        &cache,
        &Scope::github("github.invalid", Some(43)).unwrap(),
        &original.repositories
    )
    .is_err());
    assert_eq!(
        cache.draft(&scope, "review").unwrap().as_deref(),
        Some("original pending body")
    );
    assert_eq!(
        cache.saved_view(&scope, "unrelated").unwrap(),
        Some(serde_json::json!({"keep":true}))
    );
}
#[test]
fn dashboards_sql_oversized_or_invalid_preferences_never_become_default_or_broaden_scope() {
    let temp = private_temp();
    let cache = Cache::open(&temp.path().join("cache.sqlite3")).unwrap();
    let p = profile();
    let scope = p.scope().unwrap();
    for value in [
        serde_json::json!({"bad":true}),
        serde_json::json!("x".repeat(PROFILE_BYTES + 1)),
    ] {
        cache.save_view(&scope, PROFILE_KEY, &value).unwrap();
        assert!(SavedDashboardV1::load(&cache, &scope, &p.repositories).is_err());
    }
    p.save(&cache).unwrap();
    assert!(SavedDashboardV1::load(&cache, &scope, &["example/a".into()]).is_err());
}
