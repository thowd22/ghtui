//! Staged foundational catalogue contracts, no transport/auth substitutions.
use super::*;
fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
fn repository(id: u64, name: &str) -> CatalogRepository {
    CatalogRepository {
        id,
        node_id: Some(format!("R_{id}")),
        full_name: name.into(),
        description: "User-selected repo".into(),
        owner: None,
        default_branch: Some("main".into()),
        private: Some(true),
        fork: None,
        archived: None,
        language: None,
        permissions: None,
        updated_at: None,
        fetched_at: 1,
    }
}
fn snapshot(
    generation: u64,
    repositories: Vec<CatalogRepository>,
    complete: bool,
) -> CatalogSnapshot {
    CatalogSnapshot {
        scope: scope(),
        source: CatalogSource::Organization("org".into()),
        generation,
        repositories,
        loaded_pages: 1,
        max_pages: 20,
        max_repositories: 2000,
        complete,
        cancelled: false,
        reasons: if complete {
            vec![]
        } else {
            vec!["Next page rate limited; loaded subset only".into()]
        },
    }
}
fn state() -> CatalogState {
    CatalogState::new(scope(), CatalogSource::Organization("org".into()), None).unwrap()
}

#[test]
fn repo_catalog_canonical_rename_and_partial_unseen_members_keep_intent_without_false_presence() {
    let mut state = state();
    state
        .publish(snapshot(
            1,
            vec![repository(101, "org/old"), repository(102, "org/other")],
            true,
        ))
        .unwrap();
    state.toggle_selected(101).unwrap();
    state.toggle_pin(101).unwrap();
    state.save_set("important").unwrap();
    let prior = state.route(101, CatalogDestination::PullRequests).unwrap();
    state
        .publish(snapshot(2, vec![repository(102, "org/other")], false))
        .unwrap();
    assert_eq!(state.rows.len(), 2);
    assert!(!state.rows[&101].current);
    assert!(state.resolve_set("important").is_err());
    assert!(state.validate_route(&prior).is_err());
    assert_eq!(state.intent.pins[&101].last_name, "org/old");
    assert!(!state.complete);
    state
        .publish(snapshot(3, vec![repository(101, "org/renamed")], true))
        .unwrap();
    assert_eq!(state.rows.len(), 1);
    assert_eq!(
        state.resolve_set("important").unwrap()[0].full_name,
        "org/renamed"
    );
    let current = state.route(101, CatalogDestination::Actions).unwrap();
    assert_eq!(current.repository_id, 101);
    assert_eq!(current.repository, "org/renamed");
    assert!(state.validate_route(&current).is_ok());
}

#[test]
fn repo_catalog_numeric_principal_and_source_generation_do_not_unlock_old_private_intent() {
    let mut state = state();
    state
        .publish(snapshot(1, vec![repository(101, "org/private")], true))
        .unwrap();
    state.toggle_selected(101).unwrap();
    state.save_set("private-set").unwrap();
    let durable = serde_json::to_string(&state.intent).unwrap();
    let intent: CatalogIntent = serde_json::from_str(&durable).unwrap();
    let restarted = CatalogState::new(scope(), state.source.clone(), Some(intent.clone())).unwrap();
    assert!(restarted.rows.is_empty());
    assert!(restarted.resolve_set("private-set").is_err());
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(CatalogState::new(other.clone(), state.source.clone(), Some(intent)).is_err());
    let mut wrong = snapshot(2, vec![repository(101, "org/reassigned")], true);
    wrong.scope = other;
    assert!(state.publish(wrong).is_err());
    assert_eq!(state.rows[&101].repository.full_name, "org/private");
    let mut wrong_source = snapshot(2, vec![repository(101, "org/private")], true);
    wrong_source.source = CatalogSource::AuthenticatedAccessible;
    assert!(state.publish(wrong_source).is_err());
    assert!(state.publish(snapshot(1, vec![], true)).is_err());
    assert_eq!(state.generation, 1);
    assert_eq!(state.intent.sets["private-set"][&101].id, 101);
}

#[test]
fn repo_catalog_recreated_slug_nullable_metadata_and_exact_mapping_refuse_name_based_routes() {
    let mut state = state();
    state
        .publish(snapshot(1, vec![repository(101, "org/reused")], true))
        .unwrap();
    state.toggle_selected(101).unwrap();
    state.toggle_pin(101).unwrap();
    state.save_set("stable").unwrap();
    state
        .map_local(101, "/tmp/explicit-user-checkout".into())
        .unwrap();
    let route = state
        .route(
            101,
            CatalogDestination::LocalRepository {
                directory: "/tmp/explicit-user-checkout".into(),
            },
        )
        .unwrap();
    state
        .publish(snapshot(2, vec![repository(202, "org/reused")], true))
        .unwrap();
    assert!(state.validate_route(&route).is_err());
    assert!(state.resolve_set("stable").is_err());
    assert!(!state.intent.pins.contains_key(&202));
    assert!(!state.intent.mappings.contains_key(&202));
    assert!(state
        .route(
            202,
            CatalogDestination::LocalRepository {
                directory: "/tmp/explicit-user-checkout".into()
            }
        )
        .is_err());
    let labels = state.rows[&202].repository.labels();
    assert!(labels.contains("fork:unknown"));
    assert!(labels.contains("archived:unknown"));
    assert!(labels.contains("read:unknown"));
    assert!(labels.contains("unknown owner"));
    assert!(state.filtered(CatalogFilter::NotArchived).is_empty());
    assert!(state.filtered(CatalogFilter::NotForks).is_empty());
    assert_eq!(state.filtered(CatalogFilter::All).len(), 1);
    assert!(state
        .route(
            202,
            CatalogDestination::RemoteTree {
                default_branch: "invented".into()
            }
        )
        .is_err());
    assert!(state.map_local(202, "relative/path".into()).is_err());
    let mut wrong = snapshot(
        3,
        vec![
            repository(202, "org/reused"),
            repository(202, "org/different"),
        ],
        true,
    );
    assert!(state.publish(wrong.clone()).is_err());
    assert_eq!(state.generation, 2);
    wrong.repositories = vec![repository(0, "org/bad")];
    assert!(state.publish(wrong).is_err());
    let mut public = repository(203, "alice/project");
    assert!(public
        .validate(&CatalogSource::PublicUser("alice".into()))
        .is_err());
    public.private = Some(false);
    assert!(public
        .validate(&CatalogSource::PublicUser("alice".into()))
        .is_ok());
}

#[test]
fn repo_catalog_prepared_ghost_reference_remains_unavailable_and_can_be_unpinned_without_retarget()
{
    let mut state = state();
    state
        .publish(snapshot(1, vec![repository(101, "org/reused")], true))
        .unwrap();
    state.toggle_selected(101).unwrap();
    state.toggle_pin(101).unwrap();
    state.save_set("keep-original").unwrap();
    state
        .publish(snapshot(2, vec![repository(202, "org/reused")], true))
        .unwrap();
    let rows = state.prepare_rows(CatalogFilter::All);
    assert_eq!(rows.len(), 2);
    let old = rows.iter().find(|row| row.id == 101).unwrap();
    assert!(!old.current && old.pinned && old.selected);
    assert!(old.default_branch.is_none());
    assert!(old.labels.contains("never retarget by name"));
    let controls = ui::CatalogControls {
        selected: rows.iter().position(|row| row.id == 101).unwrap(),
        ..Default::default()
    }
    .controls(&state, &rows, false);
    assert!(controls
        .iter()
        .any(|control| control.action == ui::CatalogAction::Pin(101) && control.enabled));
    assert!(controls
        .iter()
        .filter(|control| matches!(control.action, ui::CatalogAction::Open(..)))
        .all(|control| !control.enabled));
    state.toggle_selected(101).unwrap();
    state.toggle_pin(101).unwrap();
    assert!(!state.selected.contains(&101));
    assert!(!state.intent.pins.contains_key(&101));
    assert!(state.resolve_set("keep-original").is_err());
}

#[test]
fn repo_catalog_prepared_visible_rows_keyboard_mouse_focus_and_busy_narrow_hits_share_identity() {
    use crate::{
        config::ThemeName,
        theme::{ColorMode, Theme},
    };
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::{backend::TestBackend, layout::Rect, Terminal};
    let mut state = state();
    state
        .publish(snapshot(
            1,
            vec![repository(101, "org/a"), repository(102, "org/b")],
            true,
        ))
        .unwrap();
    let rows = state.prepare_rows(CatalogFilter::All);
    for (width, height) in [(120, 40), (80, 24)] {
        let theme = Theme::new(ThemeName::NoColor, ColorMode::NoColor);
        let mut navigation = ui::CatalogControls::default();
        let controls = navigation.controls(&state, &rows, false);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut hits = vec![];
        terminal
            .draw(|frame| {
                hits = ui::draw_rows(
                    frame,
                    Rect::new(0, 0, width, 4),
                    &rows,
                    &mut navigation,
                    false,
                    theme,
                );
                hits.extend(ui::draw_controls(
                    frame,
                    Rect::new(0, 6, width, height - 6),
                    &controls,
                    navigation.focused,
                    theme,
                ));
            })
            .unwrap();
        let pin = hits
            .iter()
            .find(|hit| hit.action == ui::CatalogAction::Pin(101))
            .unwrap();
        let click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: pin.rect.x + 1,
            row: pin.rect.y,
            modifiers: KeyModifiers::NONE,
        });
        let mouse = navigation.event_action(&click, rows.len(), &controls, &hits);
        let keyboard = navigation.event_action(
            &Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
            rows.len(),
            &controls,
            &hits,
        );
        assert_eq!(mouse, keyboard);
        assert_eq!(mouse, Some(ui::CatalogAction::Pin(101)));
        let second = hits.iter().find(|hit| hit.row_index == Some(1)).unwrap();
        let row_click = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: second.rect.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            navigation.event_action(&row_click, rows.len(), &controls, &hits),
            Some(ui::CatalogAction::Select(102))
        );
        assert_eq!(navigation.selected, 1);
        let busy = navigation.controls(&state, &rows, true);
        assert!(busy
            .iter()
            .filter(|control| control.action != ui::CatalogAction::Close)
            .all(|control| !control.enabled));
        assert_eq!(
            navigation.event_action(
                &Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE)),
                rows.len(),
                &busy,
                &[]
            ),
            None
        );
    }
}
