use super::*;

fn filtered(mode: FilterModeV4, parts: &[&[u8]]) -> io::Result<Vec<u8>> {
    let mut filter = match mode {
        FilterModeV4::Capture => TerminalFilterV4::capture(),
        FilterModeV4::Pager => TerminalFilterV4::pager(8, 20)?,
    };
    let mut bytes = Vec::new();
    for part in parts {
        filter.feed(part, &mut bytes, 4096)?;
    }
    filter.finish()?;
    Ok(bytes)
}
#[test]
fn native_external_diff_filter_sgr_only_capture_never_passes_queries_clipboard_or_titles() {
    let bytes=filtered(FilterModeV4::Capture,&[b"old\x1b[31m-new\x1b[0m\x1b]52;c;secret\x07\x1b]0;evil-title\x1b\\\x1b[6n\x1b[8;1;1t\x1b[?1003h end"]).unwrap();
    assert_eq!(bytes, b"old\x1b[31m-new\x1b[0m end");
    assert!(!bytes.windows(2).any(|p| p == b"\x1b]"));
}
#[test]
fn native_external_diff_filter_emulates_cursor_but_strips_mouse_and_host_control_modes() {
    let bytes = filtered(
        FilterModeV4::Pager,
        &[b"\x1b[?1049h\x1b[2J\x1b[H\x1b[3;4H\x1b[?1000h\x1b[?2004h\x1b[5ntext\r\n\x1b[?1049l"],
    )
    .unwrap();
    assert_eq!(
        bytes,
        b"\x1b[?1049h\x1b[2J\x1b[H\x1b[3;4Htext\r\n\x1b[?1049l"
    );
    let mut parser = vt100::Parser::new(8, 20, 0);
    parser.process(&bytes);
    assert_eq!(parser.screen().title(), "");
    assert_eq!(
        parser.screen().mouse_protocol_mode(),
        vt100::MouseProtocolMode::None
    );
}
#[test]
fn native_external_diff_filter_utf8_and_escape_sequences_survive_every_byte_boundary() {
    let original = "界e\u{301}\x1b[38;2;10;20;30mnew\x1b[0m";
    let parts: Vec<_> = original.as_bytes().chunks(1).collect();
    assert_eq!(
        filtered(FilterModeV4::Capture, &parts).unwrap(),
        original.as_bytes()
    );
}
#[test]
fn native_external_diff_filter_bidi_and_overwrites_are_visible_not_terminal_authority() {
    let source = "one\rchanged\u{202e}evil\u{2066}two\u{0085}";
    let bytes = filtered(FilterModeV4::Capture, &[source.as_bytes()]).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "one\\rchanged\\u{202e}evil\\u{2066}two\\u{85}"
    );
    assert_eq!(safe_cell("\x1b\u{202e}x"), "\u{fffd}\u{fffd}x");
}
#[test]
fn native_external_diff_filter_refuses_incomplete_invalid_and_overlong_sequences() {
    for input in [
        &b"\x1b[31"[..],
        &b"\xf0\x9f"[..],
        &b"\xe0\x80\x80"[..],
        &b"\xff"[..],
    ] {
        assert!(filtered(FilterModeV4::Capture, &[input]).is_err());
    }
    let mut long = b"\x1b]0;".to_vec();
    long.extend_from_slice(&[b'a'; 129]);
    long.push(7);
    assert!(filtered(FilterModeV4::Pager, &[&long]).is_err());
}
#[test]
fn native_external_diff_filter_sgr_exact_whitelist_and_capacity_limit_are_enforced() {
    let bytes = filtered(
        FilterModeV4::Capture,
        &[b"\x1b[38;5;255mA\x1b[48;2;0;128;255mB\x1b[38;2;0;999;0mC\x1b[999mD"],
    )
    .unwrap();
    assert_eq!(bytes, b"\x1b[38;5;255mA\x1b[48;2;0;128;255mBCD");
    let mut filter = TerminalFilterV4::capture();
    let mut bytes = Vec::new();
    assert!(filter.feed(b"12345", &mut bytes, 4).is_err());
    assert!(bytes.capacity() <= 4);
    assert_eq!(bytes, b"1234");
}

#[test]
fn native_external_diff_filter_all_pinned_grid_commands_use_actual_geometry_and_defaults() {
    let mut commands = Vec::new();
    for final_byte in [b'@', b'P', b'X', b'C', b'D', b'G'] {
        for parameter in ["", "0", "1", "12"] {
            commands.push(format!("\x1b[{parameter}{}", char::from(final_byte)));
        }
    }
    for final_byte in [b'A', b'B', b'd', b'L', b'M', b'S', b'T'] {
        for parameter in ["", "0", "1", "4"] {
            commands.push(format!("\x1b[{parameter}{}", char::from(final_byte)));
        }
    }
    for parameters in ["", "0", "0;0", ";12", "4;", "4;12"] {
        commands.push(format!("\x1b[{parameters}H"));
    }
    for parameters in ["", "0", ";", "1;4", "0;0"] {
        commands.push(format!("\x1b[{parameters}r"));
    }
    for final_byte in [b'J', b'K'] {
        for parameter in ["", "0", "1", "2"] {
            commands.push(format!("\x1b[{parameter}{}", char::from(final_byte)));
        }
    }
    for suffix in ["?1;6;25;47;1049h", "?1049;47;25;6;1l"] {
        commands.push(format!("\x1b[{suffix}"));
    }
    commands.extend(["\x1b7".to_owned(), "\x1b8".to_owned(), "\x1bM".to_owned()]);
    for command in commands {
        let mut filter = TerminalFilterV4::pager(4, 12).unwrap();
        let mut safe = Vec::new();
        for byte in command.as_bytes().chunks(1) {
            filter.feed(byte, &mut safe, 4096).unwrap();
        }
        filter.finish().unwrap();
        assert_eq!(safe, command.as_bytes(), "{command:?}");
        let mut parser = vt100::Parser::new(4, 12, 0);
        parser.process(b"\x1b[4;12H");
        parser.process(&safe);
        assert_eq!(parser.screen().size(), (4, 12));
        let (row, col) = parser.screen().cursor_position();
        assert!(row < 4 && col <= 12, "{command:?}");
        assert_eq!(parser.screen().title(), "");
        assert_eq!(
            parser.screen().mouse_protocol_mode(),
            vt100::MouseProtocolMode::None
        );
    }
}

#[test]
fn native_external_diff_filter_refuses_overbound_edit_combinations_before_parser_admission() {
    for command in [
        "\x1b[13@",
        "\x1b[32767@",
        "\x1b[65535@",
        "\x1b[999999@",
        "\x1b[12;12@",
        "\x1b[5A",
        "\x1b[5B",
        "\x1b[5L",
        "\x1b[5M",
        "\x1b[5S",
        "\x1b[5T",
        "\x1b[5d",
        "\x1b[13P",
        "\x1b[13X",
        "\x1b[13C",
        "\x1b[13D",
        "\x1b[13G",
        "\x1b[5;12H",
        "\x1b[4;13H",
        "\x1b[4;12;1H",
        "\x1b[5;4r",
        "\x1b[0;5r",
        "\x1b[1;4;1r",
        "\x1b[3J",
        "\x1b[3K",
    ] {
        let mut filter = TerminalFilterV4::pager(4, 12).unwrap();
        let mut safe = b"previous".to_vec();
        assert!(
            filter.feed(command.as_bytes(), &mut safe, 4096).is_err(),
            "{command:?}"
        );
        assert_eq!(
            safe, b"previous",
            "the rejected control must never reach pinned vt100"
        );
    }
    let stripped=filtered(FilterModeV4::Pager,&[b"\x1b[?7;1047;1048h\x1b[?25;1000h\x1b[?2004l\x1b[1;2f\x1b[2E\x1b[2F\x1b[s\x1b[u\x1bD\x1bEvisible"]).unwrap();
    assert_eq!(
        stripped, b"visible",
        "unimplemented and host-only modes are not emulation authority"
    );
}
