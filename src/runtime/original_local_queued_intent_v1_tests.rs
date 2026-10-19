//! Actual original worker and completion channel, with a newer real UI intent.
use super::*;
use crate::{
    app::{Action, HitRegion},
    local_commit_actor_origin_v1_test_support::Real,
    repository_graph_v1::GraphActionV1,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use std::{sync::mpsc, time::Duration};

fn newer_intent(mode_change: bool) {
    for mouse in [false, true] {
        for outcome in 0..3 {
            let real = Real::new("sha256");
            let source = Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
                .unwrap()
                .unwrap();
            let mut app = App {
                view: View::Repository,
                repository_mode: RepositoryMode::Graph,
                repository_list: crate::ui_table::ListState {
                    selected: source.graph_focus_row_v1().unwrap(),
                    ..Default::default()
                },
                workspace: Some(source.clone()),
                refresh_requested: false,
                ..Default::default()
            };
            let baseline = real.snapshot();
            real.clear_trace();
            let original_graph = source.graph_state_v1.as_ref().unwrap().clone();
            let original_display = source.graph_display_v1.as_ref().unwrap().clone();
            let mut background = Background::new();
            assert!(
                background.submit_original_local_v1(&mut app, None, move |owner| {
                    match outcome {
                        0 => {
                            let mut prepared = source;
                            prepared.open_original_with_operation_v1(
                                RowAction::Graph(GraphActionV1::Horizontal(8)),
                                owner.operation(),
                            )?;
                            Ok(PreparedView {
                                workspace: Some(prepared),
                                status: "STALE_GRAPH_SUCCESS".into(),
                                ..Default::default()
                            })
                        }
                        1 => Err("STALE_GRAPH_ERROR".into()),
                        _ => panic!("STALE_GRAPH_PANIC"),
                    }
                })
            );
            // Obtain the genuine completed worker message, then deliver it
            // through the same real publisher channel after newer UI input.
            let completion = background
                .completions
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            assert!(completion
                .local_navigation_owner
                .as_ref()
                .unwrap()
                .matches_original(app.workspace.as_ref().unwrap()));
            let (sender, receiver) = mpsc::channel();
            background.completions = receiver;
            let action = if mode_change {
                Action::RepositoryMode(RepositoryMode::Tree)
            } else {
                Action::RepositoryRow(RowAction::Commit(real.original.clone()))
            };
            if mouse {
                app.hits = vec![HitRegion {
                    rect: Rect::new(2, 2, 12, 1),
                    action,
                }];
                app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 2,
                    row: 2,
                    modifiers: KeyModifiers::NONE,
                }));
            } else {
                app.handle_event(Event::Key(KeyEvent::new(
                    if mode_change {
                        KeyCode::Char('t')
                    } else {
                        KeyCode::Enter
                    },
                    KeyModifiers::NONE,
                )));
            }
            assert_eq!(app.repository_load_requested, mode_change);
            assert_eq!(
                app.repository_mode,
                if mode_change {
                    RepositoryMode::Tree
                } else {
                    RepositoryMode::Graph
                }
            );
            assert_eq!(
                app.repository_action,
                (!mode_change).then_some(RowAction::Commit(real.original.clone()))
            );
            let status = app.status.clone();
            let loading = app.loading;
            let hits = app
                .hits
                .iter()
                .map(|hit| (hit.rect, hit.action.clone()))
                .collect::<Vec<_>>();
            let selected = app.repository_list.selected;
            sender.send(completion).unwrap();
            assert!(
                !background.poll(&mut app),
                "A newer queued intent must reject old success, error and panic before all effects"
            );
            assert_eq!(app.status, status);
            assert_eq!(app.loading, loading);
            assert_eq!(
                app.hits
                    .iter()
                    .map(|hit| (hit.rect, hit.action.clone()))
                    .collect::<Vec<_>>(),
                hits
            );
            assert_eq!(app.repository_list.selected, selected);
            assert_eq!(app.repository_load_requested, mode_change);
            assert_eq!(
                app.repository_mode,
                if mode_change {
                    RepositoryMode::Tree
                } else {
                    RepositoryMode::Graph
                }
            );
            assert_eq!(
                app.repository_action,
                (!mode_change).then_some(RowAction::Commit(real.original.clone()))
            );
            let current = app.workspace.as_ref().unwrap();
            assert_eq!(current.graph_horizontal_v1, 0);
            assert!(Arc::ptr_eq(
                current.graph_state_v1.as_ref().unwrap(),
                &original_graph
            ));
            assert!(Arc::ptr_eq(
                current.graph_display_v1.as_ref().unwrap(),
                &original_display
            ));
            assert!(real.children().is_empty());
            assert_eq!(real.snapshot(), baseline);
        }
    }
}

#[test]
fn commit_graph_v2_runtime_new_mode_rejects_prior_success_error_panic_before_intent_effects() {
    newer_intent(true);
}

#[test]
fn commit_graph_v2_runtime_new_action_rejects_prior_success_error_panic_before_intent_effects() {
    newer_intent(false);
}
