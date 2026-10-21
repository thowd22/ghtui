use super::*;
use std::collections::BTreeMap;

#[test]
fn clipboard_osc52_encodes_base64_between_the_osc_and_bel() {
    assert_eq!(
        osc52_sequence("hello").unwrap(),
        b"\x1b]52;c;aGVsbG8=\x07".to_vec()
    );
    // Multi-byte text is encoded as UTF-8.
    assert_eq!(
        osc52_sequence("日本").unwrap(),
        b"\x1b]52;c;5pel5pys\x07".to_vec()
    );
    // At the cap it is sent; one byte over it is refused with the reason.
    let limit = "a".repeat(OSC52_MAX_BYTES);
    let encoded = osc52_sequence(&limit).unwrap();
    assert!(encoded.len() < 100_000, "{}", encoded.len());
    let over = "a".repeat(OSC52_MAX_BYTES + 1);
    let refused = osc52_sequence(&over).unwrap_err();
    assert!(refused.contains("74001 bytes"), "{refused}");
}

fn fake_tools(names: &[&str]) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    for name in names {
        let path = dir.path().join(name);
        std::fs::write(&path, "#!/bin/sh\ncat >/dev/null\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    // A non-executable file is never chosen.
    std::fs::write(dir.path().join("xsel"), "not a program").unwrap();
    dir
}

fn env(pairs: &[(&str, &std::ffi::OsStr)]) -> impl Fn(&str) -> Option<OsString> {
    let map: BTreeMap<String, OsString> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn clipboard_tool_follows_the_display_and_path() {
    let dir = fake_tools(&["wl-copy", "xclip"]);
    let path = dir.path().as_os_str();
    let wayland = detect_tool(env(&[
        ("PATH", path),
        ("WAYLAND_DISPLAY", "wayland-0".as_ref()),
        ("DISPLAY", ":0".as_ref()),
    ]))
    .unwrap();
    assert_eq!(wayland.name(), "wl-copy");
    assert_eq!(wayland.args, ["--foreground"]);
    let x11 = detect_tool(env(&[("PATH", path), ("DISPLAY", ":0".as_ref())])).unwrap();
    assert_eq!(x11.name(), "xclip");
    assert_eq!(x11.args, ["-quiet", "-selection", "clipboard"]);
    // No display: nothing to hand the text to (OSC 52 still works over SSH).
    if !cfg!(target_os = "macos") {
        assert_eq!(detect_tool(env(&[("PATH", path)])), None);
    }
    // xsel is not executable here, so an X11 session without xclip has no tool.
    let only_xsel = fake_tools(&[]);
    assert_eq!(
        detect_tool(env(&[
            ("PATH", only_xsel.path().as_os_str()),
            ("DISPLAY", ":0".as_ref())
        ])),
        None
    );
}

#[test]
fn clipboard_toggle_releases_and_restores_mouse_with_a_footer_indicator() {
    let mut app = App::default();
    assert!(footer_control(&app).is_none());
    let f12 = crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::F(12),
        crossterm::event::KeyModifiers::NONE,
    ));
    assert!(pre_event(&mut app, &f12));
    assert!(app.copy_v120.mouse_released);
    assert!(app.status.starts_with("Mouse released"), "{}", app.status);
    let control = footer_control(&app).unwrap();
    assert_eq!(
        (control.key.as_str(), control.label.as_str()),
        ("F12", "Mouse off")
    );
    assert!(crate::footer::global_tail(&app)
        .iter()
        .any(|tail| tail.key == "F12"));
    // While released, a drag is the terminal's: ghtui does not see or select it.
    let drag = crossterm::event::Event::Mouse(crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left),
        column: 3,
        row: 1,
        modifiers: crossterm::event::KeyModifiers::NONE,
    });
    assert!(!pre_event(&mut app, &drag));
    assert!(!app.copy_v120.selection.active());
    // The footer token runs the same toggle as the key.
    app.dispatch(control.action);
    assert!(!app.copy_v120.mouse_released);
    assert!(app.status.starts_with("Mouse restored"), "{}", app.status);
}

#[test]
fn clipboard_copy_focused_explains_when_nothing_is_focused() {
    let mut app = App {
        view: View::Repository,
        ..Default::default()
    };
    app.dispatch(Action::Clipboard(ClipboardAction::CopyFocused));
    assert_eq!(app.status, "Nothing copied • no repository is open");
    assert!(app.copy_v120.pending_osc52.is_none());
    app.view = View::Search;
    app.dispatch(Action::Clipboard(ClipboardAction::CopyFocused));
    assert!(app.status.contains("drag over text"), "{}", app.status);
    // An empty value is never sent.
    copy(&mut app, "comment body", "");
    assert_eq!(app.status, "Nothing copied • the comment body is empty");
    assert!(app.copy_v120.pending_osc52.is_none());
    copy(&mut app, "commit SHA", "0123abcd");
    assert_eq!(
        app.copy_v120.pending_osc52.as_deref(),
        Some(b"\x1b]52;c;MDEyM2FiY2Q=\x07".as_slice())
    );
    assert!(app
        .status
        .starts_with("Copied commit SHA (8 chars) via terminal OSC 52"));
}

#[test]
fn clipboard_y_in_actions_logs_copies_the_selected_log_line() {
    use crate::actions_workspace::{ActionsCells, ActionsRow, ActionsView, ActionsWorkspace};
    let mut actions = ActionsWorkspace::new(
        crate::providers::gh::HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(42),
            gh_version: "offline".into(),
        },
        vec!["example/repo".into()],
    );
    actions.view = ActionsView::Logs;
    actions.rows = ["Run tests", "ERROR fixture assertion failed   "]
        .into_iter()
        .map(|label| ActionsRow {
            label: label.into(),
            role: crate::theme::SemanticToken::Normal,
            actor: None,
            action: None,
            cells: ActionsCells::Prose,
        })
        .collect();
    actions.selected = 1;
    let mut app = App {
        view: View::Actions,
        actions: Some(actions),
        ..Default::default()
    };
    assert_eq!(
        focused_target(&app).unwrap(),
        CopyTarget::new("log line", "ERROR fixture assertion failed")
    );
    // The `y` key reaches the same copy through the app's key handling.
    app.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('y'),
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    assert!(
        app.status
            .starts_with("Copied log line (30 chars) via terminal OSC 52"),
        "{}",
        app.status
    );
    assert_eq!(
        app.copy_v120.pending_osc52.as_deref(),
        Some(
            osc52_sequence("ERROR fixture assertion failed")
                .unwrap()
                .as_slice()
        )
    );
}
