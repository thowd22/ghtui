use super::*;
use crate::{config::ThemeName, repository_commit_actor_native_v1::tests::Kit};
use ratatui::{backend::TestBackend, Terminal};
fn theme() -> Theme {
    Theme::new(ThemeName::NoColor, crate::theme::ColorMode::NoColor)
}
fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in buffer.area.y..buffer.area.bottom() {
        for x in buffer.area.x..buffer.area.right() {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}
#[test]
fn git_actor_association_native_ui_empty_owned_chooser_explains_purpose_before_any_new_read() {
    let mut k = Kit::new("sha1", false, 42);
    let mut app = crate::app::App {
        view: crate::app::View::Repository,
        workspace: Some(k.workspace.clone()),
        local_commit_actors: std::mem::take(&mut k.state),
        ..Default::default()
    };
    app.settings_host = "github.invalid".into();
    app.dispatch(crate::app::Action::LocalActor(
        LocalActorActionV1::ChooseRepository,
    ));
    let modal = app.catalogue.as_mut().unwrap();
    assert!(modal.workspace.is_none());
    assert!(modal.local_actor_purpose.is_some());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::repository_catalogue_dialogs::draw(frame, frame.area(), modal))
        .unwrap();
    let text = screen(&terminal);
    assert!(text.contains("Choose repository for saved commit"));
    assert!(text.contains("navigation intent only"));
    assert!(text.contains("no automatic discovery or clone"));
    assert!(text.contains("[s Source]"));
    assert!(!text.contains("Save set"));
    assert!(modal.hits.iter().any(
        |(_, action)| *action == crate::repository_catalogue_native::CatalogueUiAction::Source
    ));
    k.assert_reads(7);
    assert!(k.children().is_empty());
}
#[test]
fn git_actor_association_native_ui_typed_profile_keys_do_not_steal_modified_bindings() {
    for (code, action) in [
        (KeyCode::Char('R'), LocalActorActionV1::ChooseRepository),
        (KeyCode::Char('V'), LocalActorActionV1::Verify),
        (KeyCode::Char('X'), LocalActorActionV1::Clear),
        (KeyCode::F(7), LocalActorActionV1::Details),
    ] {
        assert_eq!(profile_key(code, KeyModifiers::NONE), Some(action));
        assert_eq!(profile_key(code, KeyModifiers::CONTROL), None);
        assert_eq!(profile_key(code, KeyModifiers::ALT), None);
    }
    assert_eq!(profile_key(KeyCode::Char('r'), KeyModifiers::NONE), None);
}
#[test]
fn git_actor_association_native_ui_every_toolbar_control_same_hit_and_key_at80_and120() {
    let k = Kit::new("sha1", false, 42);
    let mut layout = LocalActorLayoutV1::default();
    for width in [80, 120] {
        let mut terminal = Terminal::new(TestBackend::new(width, 4)).unwrap();
        terminal
            .draw(|frame| layout.controls(frame, frame.area(), &k.state, theme()))
            .unwrap();
        assert_eq!(layout.hits().len(), 4);
        assert!(screen(&terminal).contains("[R Choose repository]"));
        for hit in layout.hits() {
            assert_eq!(
                layout.mouse(
                    MouseEventKind::Down(MouseButton::Left),
                    hit.rect.x,
                    hit.rect.y
                ),
                Some(hit.action.clone())
            );
        }
        layout.clear();
        assert!(layout
            .mouse(MouseEventKind::Down(MouseButton::Left), 0, 0)
            .is_none());
    }
    k.assert_reads(7);
    assert!(k.children().is_empty());
}
#[test]
fn git_actor_association_native_ui_true_report_roles_keep_exact_host_and_raw_git_role_names() {
    let mut k = Kit::new("sha1", false, 42);
    let (_, report) = k.verify();
    let commit = k.workspace.selected_commit.as_ref().unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 4)).unwrap();
    let images = ActorAvatarStatesV1::default();
    terminal
        .draw(|frame| {
            for (row, role, actor) in [
                (0, GitRole::Author, &commit.author),
                (2, GitRole::Committer, &commit.committer),
            ] {
                let local = crate::git_actor_labels_v1::display_identity(actor, role);
                draw_role(
                    frame,
                    Rect::new(0, row, 120, 2),
                    role,
                    &local,
                    Some(&report),
                    &images,
                    theme(),
                );
            }
        })
        .unwrap();
    let text = screen(&terminal);
    assert!(text.contains("host-writer"));
    assert!(text.contains("host-recorder"));
    assert!(text.contains("Raw Writer"));
    assert!(text.contains("Raw Recorder"));
    assert!(!text.contains('▀'));
    assert_eq!(report.source().sha(), k.sha);
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_ui_null_role_fallback_does_not_show_other_roles_host_name() {
    let mut k = Kit::new("sha1", true, 42);
    let (_, report) = k.verify();
    let commit = k.workspace.selected_commit.as_ref().unwrap();
    let local = crate::git_actor_labels_v1::display_identity(&commit.author, GitRole::Author);
    let mut terminal = Terminal::new(TestBackend::new(80, 2)).unwrap();
    terminal
        .draw(|f| {
            draw_role(
                f,
                f.area(),
                GitRole::Author,
                &local,
                Some(&report),
                &ActorAvatarStatesV1::default(),
                theme(),
            )
        })
        .unwrap();
    let text = screen(&terminal);
    assert!(text.contains("author Raw Writer"));
    assert!(!text.contains("host-recorder"));
    assert!(!text.contains("host-writer"));
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_ui_detail_scroll_keys_mouse_resize_and_tiny_close_are_safe() {
    let mut k = Kit::new("sha1", false, 42);
    let (owner, report) = k.verify();
    assert!(k.state.accept(&owner, report));
    let mut layout = LocalActorLayoutV1::default();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| layout.details(f, f.area(), &k.state, theme()))
        .unwrap();
    assert!(screen(&terminal).contains("Saved commit host report • read-only"));
    k.state.apply(LocalActorActionV1::Focus(1)).unwrap();
    terminal
        .draw(|f| layout.details(f, f.area(), &k.state, theme()))
        .unwrap();
    assert_eq!(
        layout.focused_action(k.state.detail_focus),
        Some(LocalActorActionV1::Scroll(-3))
    );
    let focused = layout.hits()[1].rect;
    assert!(terminal.backend().buffer()[(focused.x, focused.y)]
        .modifier
        .contains(ratatui::style::Modifier::UNDERLINED));
    assert_eq!(
        layout.mouse(MouseEventKind::ScrollDown, 5, 5),
        detail_key(KeyCode::Down)
    );
    assert!(layout.mouse(MouseEventKind::ScrollDown, 79, 23).is_none());
    let old = layout
        .hits()
        .iter()
        .find(|h| h.action == LocalActorActionV1::Back)
        .unwrap()
        .rect;
    layout.clear();
    assert!(layout.focused_action(k.state.detail_focus).is_none());
    assert!(layout
        .mouse(MouseEventKind::Down(MouseButton::Left), old.x, old.y)
        .is_none());
    let mut tiny = Terminal::new(TestBackend::new(6, 1)).unwrap();
    tiny.draw(|f| layout.details(f, f.area(), &k.state, theme()))
        .unwrap();
    assert_eq!(layout.hits().len(), 1);
    assert_eq!(
        layout.mouse(MouseEventKind::Down(MouseButton::Left), 0, 0),
        Some(LocalActorActionV1::Back)
    );
    k.assert_reads(14);
}
