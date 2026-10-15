#[path = "support/cursorless_frames.rs"]
mod cursorless_frames;
#[path = "tui_e2e/repository_catalogue_close_frame_v1.rs"]
mod repository_catalogue_close_frame_v1;
use cursorless_frames::CursorlessFrames;
use repository_catalogue_close_frame_v1::restored_main;

#[test]
fn catalogue_close_frame_v1_test_framework_title_and_old_quit_are_not_completed_geometry() {
    for columns in [80, 120] {
        let mut frames = CursorlessFrames::default();
        let mut parser = vt100::Parser::new(24, columns, 0);
        let catalogue = "\x1b[1;1HRepositories • explicit discovery\x1b[12;1Hexample/repo1000\x1b[23;1H[q Quit] [Esc Close]\x1b[?25l".as_bytes();
        frames.process(&mut parser, catalogue, 0);
        let before = frames.end;
        assert!(!restored_main(frames.last.as_ref(), frames.end, before));

        // The genuine failed transcript accepts its first main title while the
        // previous catalogue footer still survives. Reproduce that ordering.
        let title = "\x1b[1;1H\x1b[2K\x1b[4;1HRepository • focused view".as_bytes();
        frames.process(&mut parser, title, catalogue.len());
        assert!(parser
            .screen()
            .contents()
            .contains("Repository • focused view"));
        assert!(parser.screen().contents().contains("[q Quit] [Esc Close]"));
        assert!(!restored_main(frames.last.as_ref(), frames.end, before));

        // Even a complete main-looking footer in a partial repaint cannot
        // replace the previous completed frame or its geometry.
        let body = b"\x1b[12;1H\x1b[2K\x1b[23;1H\x1b[2K\x1b[24;1H[? Help]  [: Commands]  [r Refresh]  [s Settings]  [q Quit]";
        frames.process(&mut parser, body, catalogue.len() + title.len());
        assert!(restored_main(Some(parser.screen()), before + 1, before));
        assert!(!restored_main(frames.last.as_ref(), frames.end, before));
        let end = catalogue.len() + title.len() + body.len();
        frames.process(&mut parser, b"\x1b[?25l", end);
        assert!(restored_main(frames.last.as_ref(), frames.end, before));
        assert!(!frames
            .last
            .as_ref()
            .unwrap()
            .contents()
            .contains("example/repo1000"));
    }
}

#[test]
fn catalogue_close_frame_v1_test_framework_old_freshness_notice_footer_and_resize_refuse() {
    let mut frames = CursorlessFrames::default();
    let mut parser = vt100::Parser::new(24, 80, 0);
    let main = "\x1b[4;1HRepository • focused view\x1b[24;1H[? Help]  [: Commands]  [r Refresh]  [s Settings]  [q Quit]\x1b[?25l".as_bytes();
    frames.process(&mut parser, main, 0);
    let before = frames.end;
    assert!(restored_main(frames.last.as_ref(), frames.end, 0));
    assert!(!restored_main(frames.last.as_ref(), frames.end, before));
    let notice = "\x1b[10;1HAction unavailable • explanation only\x1b[?25l".as_bytes();
    frames.process(&mut parser, notice, main.len());
    assert!(!restored_main(frames.last.as_ref(), frames.end, before));
    let incomplete_footer = b"\x1b[10;1H\x1b[2K\x1b[24;1H\x1b[2K[q Quit] [Esc Close]\x1b[?25l";
    frames.process(&mut parser, incomplete_footer, main.len() + notice.len());
    assert!(!restored_main(frames.last.as_ref(), frames.end, before));
    frames.invalidate_size();
    parser.set_size(40, 120);
    assert!(!restored_main(frames.last.as_ref(), frames.end, before));
}
