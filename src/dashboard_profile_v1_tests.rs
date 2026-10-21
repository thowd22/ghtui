use super::*;
use std::os::unix::fs::PermissionsExt;
fn private_temp() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn legacy_layout() -> LegacyLayoutV1 {
    LegacyLayoutV1 {
        slots: WIDGETS
            .iter()
            .filter_map(|(widget, _)| widget.metric())
            .map(|kind| WidgetSlotV1 {
                kind,
                visible: true,
            })
            .collect(),
    }
}
fn profile() -> SavedDashboardV1 {
    SavedDashboardV1 {
        kind: "ghtui-dashboard".into(),
        version: 1,
        host: "github.invalid".into(),
        account_id: 42,
        repositories: vec!["example/a".into(), "example/b".into()],
        window_days: 7,
        layout: legacy_layout(),
    }
}
fn named(name: &str) -> NamedDashboard {
    NamedDashboard {
        name: name.into(),
        repositories: vec!["example/a".into()],
        window_days: 30,
        stale_days: 14,
        layout: Default::default(),
    }
}
fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
#[test]
fn dashboards_layout_reorders_hides_and_refuses_last_visible_or_unknown_widget() {
    let mut layout = DashboardLayout::default();
    assert_eq!(layout.slots.len(), 10);
    assert_eq!(layout.move_slot(0, true).unwrap(), 1);
    assert_eq!(layout.shown().next(), Some(DashboardWidget::NeedsMyReview));
    layout.toggle(1).unwrap();
    assert!(!layout
        .shown()
        .any(|widget| widget == DashboardWidget::OpenPrs));
    // The attention widgets are slots like the metric cards.
    assert_eq!(layout.move_slot(9, false).unwrap(), 8);
    assert_eq!(layout.slots[8].widget, DashboardWidget::UnresolvedThreads);
    for index in 1..10 {
        if layout.slots[index].visible {
            layout.toggle(index).unwrap();
        }
    }
    let before = layout.clone();
    assert!(layout.toggle(0).is_err());
    assert_eq!(layout, before);
    assert!(layout.move_slot(10, false).is_err());
    assert_eq!(layout.move_slot(0, false).unwrap(), 0);
    assert_eq!(layout.move_slot(9, true).unwrap(), 9);
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
fn dashboards_version1_default_reads_as_named_default_and_is_never_rewritten() {
    let temp = private_temp();
    let cache = Cache::open(&temp.path().join("cache.sqlite3")).unwrap();
    let scope = scope();
    let mut original = profile();
    original.layout.slots.swap(0, 1);
    original.layout.slots[1].visible = false;
    let legacy = serde_json::to_value(&original).unwrap();
    cache.save_view(&scope, PROFILE_KEY, &legacy).unwrap();
    let saved = SavedDashboards::load(&cache, &scope).unwrap().unwrap();
    assert_eq!(saved.active(), Some(DEFAULT_NAME));
    let restored = saved
        .restore(None, &["example/a".into(), "example/b".into()])
        .unwrap();
    assert_eq!(restored.name(), DEFAULT_NAME);
    assert_eq!(restored.window_days(), 7);
    assert_eq!(restored.stale_days(), DEFAULT_STALE_DAYS);
    let order = restored
        .layout()
        .slots()
        .iter()
        .map(|slot| (slot.widget, slot.visible))
        .collect::<Vec<_>>();
    assert_eq!(order[0], (DashboardWidget::NeedsMyReview, true));
    assert_eq!(order[1], (DashboardWidget::OpenPrs, false));
    assert_eq!(order[8], (DashboardWidget::StalePrs, true));
    assert_eq!(order[9], (DashboardWidget::UnresolvedThreads, true));
    // A saved scope never widens the declared one.
    assert!(saved
        .restore(None, &["example/a".into()])
        .unwrap_err()
        .contains("Saved scope outside"));
    // Saving a second dashboard writes version 2 and leaves version 1 as it was.
    let mut write = SavedDashboards::load_for_write(&cache, &scope).unwrap();
    write.create(named("Team")).unwrap();
    write.save(&cache).unwrap();
    assert_eq!(cache.saved_view(&scope, PROFILE_KEY).unwrap(), Some(legacy));
    let saved = SavedDashboards::load(&cache, &scope).unwrap().unwrap();
    assert_eq!(
        saved
            .entries()
            .iter()
            .map(|entry| (entry.name.as_str(), entry.active))
            .collect::<Vec<_>>(),
        [(DEFAULT_NAME, false), ("Team", true)]
    );
}
#[test]
fn dashboards_named_create_rename_switch_delete_refuse_duplicates_and_bounds() {
    let mut saved = SavedDashboards::empty(&scope()).unwrap();
    assert!(saved.restore(None, &["example/a".into()]).is_err());
    saved.create(named("Team")).unwrap();
    saved.create(named("Ops")).unwrap();
    assert_eq!(saved.active(), Some("Ops"));
    assert!(saved.create(named("team")).unwrap_err().contains("exists"));
    saved.activate("TEAM").unwrap();
    assert_eq!(saved.active(), Some("Team"));
    assert_eq!(
        saved
            .restore(Some("ops"), &["example/a".into()])
            .unwrap()
            .name(),
        "Ops"
    );
    assert!(saved.rename("Team", "Ops").unwrap_err().contains("exists"));
    assert!(saved.rename("Team", "  ").is_err());
    assert!(saved.rename("Team", &"x".repeat(41)).is_err());
    assert!(saved.rename("Team", "bad\u{1b}name").is_err());
    assert_eq!(saved.rename("Team", "  Review queue ").unwrap(), "Team");
    assert_eq!(saved.active(), Some("Review queue"));
    // Saving over an existing name replaces it and keeps its spelling.
    let mut changed = named("review QUEUE");
    changed.stale_days = 3;
    saved.upsert(changed).unwrap();
    assert_eq!(saved.dashboards().len(), 2);
    assert_eq!(saved.dashboards()[0].name(), "Review queue");
    assert_eq!(saved.dashboards()[0].stale_days(), 3);
    assert_eq!(saved.delete("review queue").unwrap(), "Review queue");
    assert_eq!(saved.active(), None);
    assert!(saved.delete("missing").is_err());
    assert_eq!(
        saved.restore(None, &["example/a".into()]).unwrap().name(),
        "Ops"
    );
    for index in 0..MAX_DASHBOARDS - 1 {
        saved.create(named(&format!("Board {index}"))).unwrap();
    }
    assert!(saved
        .create(named("One too many"))
        .unwrap_err()
        .contains("At most"));
    let mut bad = named("Bad");
    bad.stale_days = 5;
    assert!(bad.validate_for(&["example/a".into()]).is_err());
}
#[test]
fn dashboards_sql_named_roundtrip_account_isolation_preserves_drafts_and_other_views() {
    let temp = private_temp();
    let path = temp.path().join("cache.sqlite3");
    let scope = scope();
    let cache = Cache::open(&path).unwrap();
    cache
        .put_draft(&scope, "review", "original pending body")
        .unwrap();
    cache
        .save_view(&scope, "unrelated", &serde_json::json!({"keep":true}))
        .unwrap();
    let mut original = SavedDashboards::empty(&scope).unwrap();
    let mut team = named("Team");
    team.layout.move_slot(8, false).unwrap();
    team.layout.toggle(0).unwrap();
    original.create(team).unwrap();
    original.save(&cache).unwrap();
    drop(cache);
    let cache = Cache::open(&path).unwrap();
    assert_eq!(
        SavedDashboards::load(&cache, &scope).unwrap(),
        Some(original.clone())
    );
    let value = cache.saved_view(&scope, PROFILES_KEY).unwrap().unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(value["dashboards"][0]["name"], "Team");
    assert_eq!(value["dashboards"][0]["stale_days"], 14);
    assert_eq!(
        value["dashboards"][0]["layout"]["slots"][7],
        serde_json::json!({"widget":"StalePrs","visible":true})
    );
    assert!(value.get("cached_metrics").is_none());
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert_eq!(SavedDashboards::load(&cache, &other).unwrap(), None);
    // Another account's record copied into this scope is refused, never adopted.
    cache.save_view(&other, PROFILES_KEY, &value).unwrap();
    assert!(SavedDashboards::load(&cache, &other).is_err());
    assert!(SavedDashboards::load_for_write(&cache, &other).is_err());
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
        assert!(SavedDashboards::load(&cache, &scope).is_err());
        // An unreadable version-1 default does not block a first version-2 save.
        assert_eq!(
            SavedDashboards::load_for_write(&cache, &scope).unwrap(),
            SavedDashboards::empty(&scope).unwrap()
        );
    }
    cache
        .save_view(&scope, PROFILE_KEY, &serde_json::to_value(&p).unwrap())
        .unwrap();
    assert!(SavedDashboards::load(&cache, &scope)
        .unwrap()
        .unwrap()
        .restore(None, &["example/a".into()])
        .is_err());
    // An unreadable version-2 record is never overwritten.
    for value in [
        serde_json::json!({"kind":"ghtui-dashboards","version":3}),
        serde_json::json!("x".repeat(PROFILES_BYTES + 1)),
    ] {
        cache.save_view(&scope, PROFILES_KEY, &value).unwrap();
        assert!(SavedDashboards::load(&cache, &scope).is_err());
        assert!(SavedDashboards::load_for_write(&cache, &scope)
            .unwrap_err()
            .contains("not overwritten"));
    }
}
