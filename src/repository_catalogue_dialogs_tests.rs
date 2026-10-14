//! Six SOURCE-ONLY meaningful renderer/input contracts; no execution claim.
use super::*;
use crate::{
    app::App,
    providers::{
        gh::{GithubProvider, RepositoryCatalogueLimits, RepositoryCatalogueSource},
        runner::CancellationToken,
    },
    repository_catalogue_native::{self as native, CatalogueWork},
    repository_catalogue_workspace_v1::{CatalogueDestination, CatalogueWorkspace},
    test_support::{Fixture, GhStep},
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::{json, Value};
use std::sync::Arc;

fn app() -> App {
    let mut app = App {
        no_color: true,
        ..App::default()
    };
    app.settings_host = "github.invalid".into();
    native::dispatch(&mut app, CatalogueUiAction::Open);
    assert!(app.catalogue.is_some());
    app
}
fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Option<CatalogueUiAction> {
    let action = key(
        app.catalogue.as_mut().unwrap(),
        KeyEvent::new(code, modifiers),
    );
    if let Some(action) = &action {
        native::dispatch(app, action.clone());
    }
    action
}
fn draw_app(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            draw(
                frame,
                Rect::new(0, 2, width, height.saturating_sub(2)),
                app.catalogue.as_mut().unwrap(),
            )
        })
        .unwrap();
    terminal
}
fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|row| {
            let mut column = 0;
            let mut text = String::new();
            while column < buffer.area.width {
                let symbol = buffer[(column, row)].symbol();
                text.push_str(symbol);
                // A wide glyph occupies continuation cells, not literal text
                // spaces. Preserve all other painted whitespace exactly.
                column =
                    column.saturating_add(ratatui::text::Span::raw(symbol).width().max(1) as u16);
            }
            text
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn event(rect: Rect, kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }
}
fn hit(app: &App, action: &CatalogueUiAction) -> Rect {
    app.catalogue
        .as_ref()
        .unwrap()
        .hits
        .iter()
        .find(|(_, candidate)| candidate == action)
        .map(|(rect, _)| *rect)
        .expect("Required actual visible action geometry")
}
fn source_editor(app: &App) -> &TextBuffer {
    let Some(CatalogueDialog::Source { editor, .. }) = &app.catalogue.as_ref().unwrap().dialog
    else {
        panic!("Actual source dialogue required")
    };
    editor
}

#[test]
fn repo_catalog_dialogs_safe_shell_only_actual_keyboard_target_is_underlined() {
    for mode in [
        crate::theme::ColorMode::TrueColor,
        crate::theme::ColorMode::NoColor,
    ] {
        for (width, height) in [(80, 24), (120, 40)] {
            for fenced in [false, true] {
                let mut app = app();
                let modal = app.catalogue.as_mut().unwrap();
                modal.theme = crate::theme::Theme::new(crate::config::ThemeName::Dark, mode);
                modal.fenced = fenced;
                modal.busy = !fenced;
                modal.shell_focus = 0;
                for expected in [CatalogueUiAction::Close, CatalogueUiAction::Quit] {
                    let terminal = draw_app(&mut app, width, height);
                    for candidate in [CatalogueUiAction::Close, CatalogueUiAction::Quit] {
                        let rect = hit(&app, &candidate);
                        assert_eq!(
                            terminal.backend().buffer()[(rect.x, rect.y)]
                                .modifier
                                .contains(Modifier::UNDERLINED),
                            candidate == expected,
                            "Only the genuine Enter target may claim visible keyboard focus"
                        );
                        assert_eq!(
                            mouse(
                                app.catalogue.as_ref().unwrap(),
                                event(rect, MouseEventKind::Down(MouseButton::Left))
                            ),
                            Some(candidate)
                        );
                    }
                    assert_eq!(
                        key(
                            app.catalogue.as_mut().unwrap(),
                            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
                        ),
                        Some(expected)
                    );
                    assert_eq!(
                        key(
                            app.catalogue.as_mut().unwrap(),
                            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)
                        ),
                        None
                    );
                    assert!(
                        app.catalogue.as_ref().unwrap().hits.is_empty(),
                        "Tab retires old geometry before redraw"
                    );
                }
                assert!(
                    !app.quit,
                    "Inspecting typed semantics never invokes an action"
                );
            }
        }
    }
}

#[test]
fn repo_catalog_dialogs_source_kind_tab_owner_submit_and_click_key_parity() {
    let mut app = app();
    assert_eq!(
        press(&mut app, KeyCode::Char('s'), KeyModifiers::NONE),
        Some(CatalogueUiAction::Source)
    );
    for (code, value, label) in [
        ('o', 0, "[o Organization]"),
        ('u', 1, "[u My accessible repositories]"),
        ('p', 2, "[p Public user]"),
    ] {
        let terminal = draw_app(&mut app, 80, 24);
        assert!(screen(&terminal).contains(label));
        let rect = hit(&app, &CatalogueUiAction::Kind(value));
        assert_eq!(
            mouse(
                app.catalogue.as_ref().unwrap(),
                event(rect, MouseEventKind::Down(MouseButton::Left))
            ),
            Some(CatalogueUiAction::Kind(value))
        );
        assert_eq!(
            press(&mut app, KeyCode::Char(code), KeyModifiers::NONE),
            Some(CatalogueUiAction::Kind(value))
        );
    }
    press(&mut app, KeyCode::Char('o'), KeyModifiers::NONE);
    assert_eq!(
        press(&mut app, KeyCode::Tab, KeyModifiers::NONE),
        Some(CatalogueUiAction::Edit)
    );
    assert!(paste(app.catalogue.as_mut().unwrap(), "example").is_none());
    assert_eq!(source_editor(&app).text, "example");
    let terminal = draw_app(&mut app, 80, 24);
    assert!(screen(&terminal).contains("Owner: example"));
    let submit = hit(&app, &CatalogueUiAction::Submit);
    assert_eq!(
        mouse(
            app.catalogue.as_ref().unwrap(),
            event(submit, MouseEventKind::Down(MouseButton::Left))
        ),
        Some(CatalogueUiAction::Submit)
    );
    assert_eq!(
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE),
        Some(CatalogueUiAction::Submit)
    );
    let modal = app.catalogue.as_ref().unwrap();
    assert!(modal.busy);
    assert!(
        matches!(&modal.request,Some(CatalogueWork::Load {source:RepositoryCatalogueSource::Organization(owner),..}) if owner=="example")
    );
}

#[test]
fn repo_catalog_dialogs_unicode_grapheme_viewport_caret_and_preinsert_caps_preserve_bytes() {
    let mut app = app();
    native::dispatch(&mut app, CatalogueUiAction::Source);
    native::dispatch(&mut app, CatalogueUiAction::Edit);
    let raw = format!("{}👩🏽‍💻e\u{301}", "界".repeat(24));
    paste(app.catalogue.as_mut().unwrap(), &raw);
    assert_eq!(source_editor(&app).text, raw);
    let terminal = draw_app(&mut app, 48, 24);
    assert!(screen(&terminal).contains("Repository discovery source"));
    let editor = app.catalogue.as_ref().unwrap().editor_area;
    assert!(source_editor(&app).left_column > 0);
    let action = mouse(
        app.catalogue.as_ref().unwrap(),
        event(editor, MouseEventKind::Down(MouseButton::Left)),
    )
    .unwrap();
    assert_eq!(
        action,
        CatalogueUiAction::EditorClick {
            column: 0,
            extend: false
        }
    );
    native::dispatch(&mut app, action);
    let actual = source_editor(&app);
    assert!(actual.text.is_char_boundary(actual.cursor));
    assert!(actual.position().1 >= actual.left_column);
    press(&mut app, KeyCode::End, KeyModifiers::NONE);
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(source_editor(&app).text, format!("{}👩🏽‍💻", "界".repeat(24)));
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(source_editor(&app).text, "界".repeat(24));
    let before = source_editor(&app).text.clone();
    paste(app.catalogue.as_mut().unwrap(), &"x".repeat(101));
    paste(app.catalogue.as_mut().unwrap(), "private\ncanary");
    assert_eq!(source_editor(&app).text, before);
    assert!(!app.catalogue.as_ref().unwrap().status.contains("canary"));
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    paste(app.catalogue.as_mut().unwrap(), "oqup");
    assert_eq!(source_editor(&app).text, "oqup");
    assert_eq!(
        press(&mut app, KeyCode::Char('q'), KeyModifiers::NONE),
        None
    );
    assert_eq!(source_editor(&app).text, "oqupq");
    assert!(!app.quit);
    assert_eq!(
        key(
            app.catalogue.as_mut().unwrap(),
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)
        ),
        Some(CatalogueUiAction::Quit)
    );
}

#[test]
fn repo_catalog_dialogs_save_activate_names_pages_focus_and_hover_wheel_are_exact() {
    let mut app = app();
    app.catalogue.as_mut().unwrap().dialog = Some(CatalogueDialog::Save {
        editor: TextBuffer::default(),
    });
    paste(app.catalogue.as_mut().unwrap(), "集中-界");
    assert!(
        matches!(&app.catalogue.as_ref().unwrap().dialog,Some(CatalogueDialog::Save {editor}) if editor.text=="集中-界")
    );
    let terminal = draw_app(&mut app, 80, 24);
    assert!(screen(&terminal).contains("Set name: 集中-界"));
    press(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    paste(app.catalogue.as_mut().unwrap(), &"a".repeat(128));
    paste(app.catalogue.as_mut().unwrap(), "b");
    let Some(CatalogueDialog::Save { editor }) = &app.catalogue.as_ref().unwrap().dialog else {
        panic!()
    };
    assert_eq!(editor.text, "a".repeat(128));
    let names = (0..30)
        .map(|index| format!("ExactCase-{index:03}-界"))
        .collect::<Vec<_>>();
    app.catalogue.as_mut().unwrap().dialog = Some(CatalogueDialog::Activate {
        names: names.clone().into(),
        selected: 0,
        top: 0,
    });
    app.catalogue.as_mut().unwrap().shell_focus = 0;
    let terminal = draw_app(&mut app, 80, 24);
    assert!(screen(&terminal).contains("> ExactCase-000-界"));
    let page_action = mouse(
        app.catalogue.as_ref().unwrap(),
        event(
            hit(&app, &CatalogueUiAction::DialogItem(16)),
            MouseEventKind::Down(MouseButton::Left),
        ),
    );
    assert_eq!(page_action, Some(CatalogueUiAction::DialogItem(16)));
    assert_eq!(
        press(&mut app, KeyCode::PageDown, KeyModifiers::NONE),
        Some(CatalogueUiAction::DialogItem(16))
    );
    let terminal = draw_app(&mut app, 80, 24);
    assert!(screen(&terminal).contains("> ExactCase-016-界"));
    let modal = app.catalogue.as_ref().unwrap();
    assert_eq!(
        mouse(modal, event(modal.dialog_area, MouseEventKind::ScrollDown)),
        Some(CatalogueUiAction::DialogScroll(3))
    );
    assert_eq!(
        mouse(
            modal,
            MouseEvent {
                column: 0,
                row: 0,
                ..event(modal.dialog_area, MouseEventKind::ScrollDown)
            }
        ),
        None
    );
    assert_eq!(press(&mut app, KeyCode::Tab, KeyModifiers::NONE), None);
    let terminal = draw_app(&mut app, 80, 24);
    let rect = hit(&app, &CatalogueUiAction::Submit);
    assert!(terminal.backend().buffer()[(rect.x, rect.y)]
        .modifier
        .contains(Modifier::UNDERLINED));
    let Some(CatalogueDialog::Activate {
        names: actual,
        selected,
        ..
    }) = &app.catalogue.as_ref().unwrap().dialog
    else {
        panic!()
    };
    assert_eq!(actual.as_ref(), names);
    assert_eq!(*selected, 16);
}

#[test]
fn repo_catalog_dialogs_message_capture_resize_and_cleared_geometry_never_leak_hits() {
    let mut app = app();
    app.catalogue.as_mut().unwrap().dialog = Some(CatalogueDialog::Message {
        reason: (0..40)
            .map(|index| format!("Reason line {index}\n"))
            .collect(),
        scroll: 0,
    });
    let terminal = draw_app(&mut app, 120, 40);
    assert!(screen(&terminal).contains("Reason line 0"));
    let old = hit(&app, &CatalogueUiAction::DialogClose);
    assert_eq!(
        press(&mut app, KeyCode::Down, KeyModifiers::NONE),
        Some(CatalogueUiAction::DialogScroll(1))
    );
    let terminal = draw_app(&mut app, 80, 24);
    assert!(!screen(&terminal).contains("Reason line 0"));
    assert!(screen(&terminal).contains("Reason line 1"));
    clear_layout(app.catalogue.as_mut().unwrap());
    assert_eq!(
        mouse(
            app.catalogue.as_ref().unwrap(),
            event(old, MouseEventKind::Down(MouseButton::Left))
        ),
        None
    );
    assert_eq!(
        mouse(
            app.catalogue.as_ref().unwrap(),
            MouseEvent {
                column: 1,
                row: 0,
                ..event(old, MouseEventKind::Down(MouseButton::Left))
            }
        ),
        None
    );
    assert_eq!(
        key(
            app.catalogue.as_mut().unwrap(),
            KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)
        ),
        None
    );
    assert_eq!(
        key(
            app.catalogue.as_mut().unwrap(),
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
        ),
        Some(CatalogueUiAction::DialogClose)
    );
}

#[test]
fn repo_catalog_dialogs_busy_and_privacy_shell_have_only_safe_focusable_clickable_quit_close() {
    for fenced in [false, true] {
        let mut app = app();
        let modal = app.catalogue.as_mut().unwrap();
        modal.busy = !fenced;
        modal.fenced = fenced;
        modal.status = "PRIVATE_SCOPE_CANARY".into();
        modal.dialog = Some(CatalogueDialog::Message {
            reason: "PRIVATE_REASON_CANARY".into(),
            scroll: 0,
        });
        let terminal = draw_app(&mut app, 80, 24);
        let body = screen(&terminal);
        assert!(body.contains("[q Quit]"));
        assert!(!body.contains("CANARY"));
        assert!(!body.contains("[s Source]"));
        assert!(app
            .catalogue
            .as_ref()
            .unwrap()
            .hits
            .iter()
            .all(|(_, action)| matches!(
                action,
                CatalogueUiAction::Close | CatalogueUiAction::Quit
            )));
        assert_eq!(
            key(
                app.catalogue.as_mut().unwrap(),
                KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)
            ),
            None
        );
        assert_eq!(press(&mut app, KeyCode::Tab, KeyModifiers::NONE), None);
        let terminal = draw_app(&mut app, 80, 24);
        let quit = hit(&app, &CatalogueUiAction::Quit);
        assert!(terminal.backend().buffer()[(quit.x, quit.y)]
            .modifier
            .contains(Modifier::UNDERLINED));
        assert_eq!(
            mouse(
                app.catalogue.as_ref().unwrap(),
                event(quit, MouseEventKind::Down(MouseButton::Left))
            ),
            Some(CatalogueUiAction::Quit)
        );
        assert_eq!(
            key(
                app.catalogue.as_mut().unwrap(),
                KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
            ),
            Some(CatalogueUiAction::Quit)
        );
        assert_eq!(
            key(
                app.catalogue.as_mut().unwrap(),
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
            ),
            Some(CatalogueUiAction::Close)
        );
        assert_eq!(paste(app.catalogue.as_mut().unwrap(), "ignored"), None);
    }
}

fn actual_workspace() -> (Fixture, CatalogueWorkspace) {
    let fixture = Fixture::new();
    let get = |endpoint: &str, value: Value| {
        GhStep::read(
            &[
                "api",
                endpoint,
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
            ],
            &value.to_string(),
        )
    };
    let account = || {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
            get("user", json!({"id":42,"login":"octocat"})),
        ]
    };
    let owner =
        json!({"id":71,"node_id":"OWNER_dialog_71","login":"example","type":"Organization"});
    let mut steps = account();
    steps.push(get("orgs/example", owner.clone()));
    steps.push(get("orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",json!([{"id":410,"node_id":"REPO_dialog_410","full_name":"example/dialog-repo","owner":owner.clone(),"permissions":{"pull":true},"private":true}])));
    steps.push(get("orgs/example", owner));
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let source = provider
        .repository_catalogue_read(
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    let (mut workspace, job) = CatalogueWorkspace::new(Arc::new(source)).unwrap();
    workspace
        .accept(job.prepare(&CancellationToken::default()).unwrap())
        .unwrap();
    workspace.focus = CatalogueFocus::List;
    (fixture, workspace)
}
#[test]
fn repo_catalog_dialogs_prepared_workspace_controls_forward_identical_keys_hits_without_requests() {
    let (fixture, workspace) = actual_workspace();
    let before = std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows = std::str::from_utf8(&before)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 7);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    let mut app = app();
    app.catalogue.as_mut().unwrap().workspace = Some(workspace);
    app.catalogue.as_mut().unwrap().status =
        "Saved observed set: exact-dialog • originals kept".into();
    for (width, height) in [(120, 40), (80, 24)] {
        let terminal = draw_app(&mut app, width, height);
        let body = screen(&terminal);
        assert!(body.contains("example/dialog-repo"));
        assert!(body.contains("Saved observed set: exact-dialog"));
        assert!(body.contains("[q Quit]"));
        let controls = app
            .catalogue
            .as_ref()
            .unwrap()
            .workspace
            .as_ref()
            .unwrap()
            .hits
            .clone();
        let mut matched = 0;
        for hit in controls {
            let pair = match hit.input {
                CatalogueInput::Source => Some((KeyCode::Char('s'), KeyModifiers::NONE)),
                CatalogueInput::QueryFocus => Some((KeyCode::Char('/'), KeyModifiers::NONE)),
                CatalogueInput::Select => Some((KeyCode::Char(' '), KeyModifiers::NONE)),
                CatalogueInput::Pin => Some((KeyCode::Char('p'), KeyModifiers::NONE)),
                CatalogueInput::SaveSet => Some((KeyCode::Char('w'), KeyModifiers::NONE)),
                CatalogueInput::ActivateSet => Some((KeyCode::Char('a'), KeyModifiers::NONE)),
                CatalogueInput::Retry => Some((KeyCode::Char('r'), KeyModifiers::NONE)),
                CatalogueInput::Open(CatalogueDestination::MyPrs) => {
                    Some((KeyCode::Enter, KeyModifiers::NONE))
                }
                CatalogueInput::Open(CatalogueDestination::Actions) => {
                    Some((KeyCode::Char('A'), KeyModifiers::NONE))
                }
                CatalogueInput::Clear => Some((KeyCode::Char('u'), KeyModifiers::CONTROL)),
                CatalogueInput::Page(-1) => Some((KeyCode::PageUp, KeyModifiers::NONE)),
                CatalogueInput::Page(1) => Some((KeyCode::PageDown, KeyModifiers::NONE)),
                CatalogueInput::DetailScroll(delta) => Some((
                    if delta < 0 {
                        KeyCode::PageUp
                    } else {
                        KeyCode::PageDown
                    },
                    KeyModifiers::CONTROL,
                )),
                CatalogueInput::Close => Some((KeyCode::Esc, KeyModifiers::NONE)),
                _ => None,
            };
            if let Some((code, mods)) = pair {
                let expected = input_action(hit.input);
                assert_eq!(
                    mouse(
                        app.catalogue.as_ref().unwrap(),
                        event(hit.rect, MouseEventKind::Down(MouseButton::Left))
                    ),
                    Some(expected.clone())
                );
                assert_eq!(
                    key(app.catalogue.as_mut().unwrap(), KeyEvent::new(code, mods)),
                    Some(expected)
                );
                matched += 1;
            }
        }
        assert!(
            matched >= 10,
            "Actual rendered keyboard/mouse controls, not a startup snapshot"
        );
        clear_layout(app.catalogue.as_mut().unwrap());
        assert_eq!(
            mouse(
                app.catalogue.as_ref().unwrap(),
                event(
                    Rect::new(1, 3, 1, 1),
                    MouseEventKind::Down(MouseButton::Left)
                )
            ),
            None
        );
    }
    assert_eq!(
        std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before
    );
}

#[test]
fn repo_catalog_dialogs_activate_page_click_and_key_keep_same_visible_original_choice() {
    for (width, height) in [(80, 24), (120, 40)] {
        let names = (0..40)
            .map(|index| format!("Original-set-{index:03}"))
            .collect::<Vec<_>>();
        let mut keyboard = app();
        let mut pointer = app();
        for app in [&mut keyboard, &mut pointer] {
            app.catalogue.as_mut().unwrap().dialog = Some(CatalogueDialog::Activate {
                names: names.clone().into(),
                selected: 0,
                top: 0,
            });
            draw_app(app, width, height);
        }
        let page = usize::from(
            pointer
                .catalogue
                .as_ref()
                .unwrap()
                .dialog_area
                .height
                .saturating_sub(4)
                .max(1),
        );
        let rect = hit(&pointer, &CatalogueUiAction::DialogItem(page));
        let action = mouse(
            pointer.catalogue.as_ref().unwrap(),
            event(rect, MouseEventKind::Down(MouseButton::Left)),
        )
        .unwrap();
        native::dispatch(&mut pointer, action);
        press(&mut keyboard, KeyCode::PageDown, KeyModifiers::NONE);
        let state = |app: &App| match &app.catalogue.as_ref().unwrap().dialog {
            Some(CatalogueDialog::Activate { selected, top, .. }) => (*selected, *top),
            _ => panic!("Actual owned selection dialogue"),
        };
        assert_eq!(state(&keyboard), state(&pointer));
        assert_eq!(state(&pointer).0, page);
        let selected = format!("> {}", names[page]);
        assert!(screen(&draw_app(&mut keyboard, width, height)).contains(&selected));
        assert!(screen(&draw_app(&mut pointer, width, height)).contains(&selected));
        assert!(native::take_request(&mut keyboard).is_none());
        assert!(native::take_request(&mut pointer).is_none());
    }
}
