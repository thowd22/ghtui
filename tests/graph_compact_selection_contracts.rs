#[path = "support/graph_compact_selection_v1.rs"]
mod selection;

const SHA: &str = "1234567890123456789012345678901234567890";
fn frame(subject: &str) -> vt100::Screen {
    let mut parser = vt100::Parser::new(40, 120, 0);
    parser.process(
        format!("Viewed: {SHA}\r\n[Home First loaded]\r\n│\x1b[7m{subject}\x1b[0m\r\n\x1b[?25l")
            .as_bytes(),
    );
    parser.screen().clone()
}
#[test]
fn commit_graph_v2_selection_actual_inverted_subject_batches8_not199_serial_roundtrips() {
    let plan = selection::first_page_down_batch(
        &frame("Ordinary graph version 09999 • author Writer"),
        "Ordinary graph version 09800",
        SHA,
    )
    .unwrap()
    .unwrap();
    assert_eq!(plan.count, 8);
    assert_eq!(plan.bytes(), b"\x1b[B".repeat(8));
    assert_eq!(plan.expected_subject, "Ordinary graph version 09991");
    let mut current = 9999;
    let mut total = 0;
    let mut batches = 0;
    loop {
        let Some(plan) = selection::first_page_down_batch(
            &frame(&format!(
                "Ordinary graph version {current:05} • actual inverse"
            )),
            "Ordinary graph version 09800",
            SHA,
        )
        .unwrap() else {
            break;
        };
        assert!(plan.count <= 8);
        total += plan.count;
        batches += 1;
        current -= plan.count;
        assert_eq!(
            plan.expected_subject,
            format!("Ordinary graph version {current:05}")
        );
    }
    assert_eq!(current, 9800);
    assert_eq!(total, 199, "one Down per linear commit; none is skipped");
    assert_eq!(batches, 25);
}
#[test]
fn commit_graph_v2_selection_last_even_batch_exact_target_then_zero_extra_keys() {
    let plan = selection::first_page_down_batch(
        &frame("Ordinary graph version 09803 • author Writer"),
        "Ordinary graph version 09800",
        SHA,
    )
    .unwrap()
    .unwrap();
    assert_eq!(plan.count, 3);
    assert_eq!(plan.expected_subject, "Ordinary graph version 09800");
    assert!(selection::first_page_down_batch(
        &frame("Ordinary graph version 09800 • author Writer"),
        "Ordinary graph version 09800",
        SHA
    )
    .unwrap()
    .is_none());
}
#[test]
fn commit_graph_v2_selection_refuses_parent_row_no_inverse_and_multiple_inverse() {
    assert!(selection::selected_subject(&frame("↳ parent 1 → 12345678")).is_err());
    let mut parser = vt100::Parser::new(40, 120, 0);
    parser.process("│Ordinary graph version 09999\r\n".as_bytes());
    assert!(selection::selected_subject(parser.screen()).is_err());
    parser.process("│\x1b[7mOrdinary graph version 09998\x1b[0m\r\n│\x1b[7mOrdinary graph version 09997\x1b[0m\r\n".as_bytes());
    assert!(selection::selected_subject(parser.screen())
        .unwrap_err()
        .contains("Ambiguous"));
    assert!(selection::selected_subject(&frame(
        "Ordinary graph version 09999 • Ordinary graph version 09998"
    ))
    .unwrap_err()
    .contains("Ambiguous"));
}
#[test]
fn commit_graph_v2_selection_refuses_wrong_direction_outside_page_and_malformed_identity() {
    for (current, target) in [("09900", "09999"), ("09799", "09800"), ("09999", "09799")] {
        assert!(selection::first_page_down_batch(
            &frame(&format!("Ordinary graph version {current} • actor")),
            &format!("Ordinary graph version {target}"),
            SHA
        )
        .is_err());
    }
    for subject in [
        "Ordinary graph version 9999",
        "Ordinary graph version 099991",
        "Ordinary graph version 0999x",
        "prefix unrelated",
    ] {
        assert!(selection::first_page_down_batch(
            &frame(subject),
            "Ordinary graph version 09800",
            SHA
        )
        .is_err());
    }
}
#[test]
fn commit_graph_v2_selection_refuses_busy_wrong_original_sha_and_controls_without_source() {
    let mut busy = vt100::Parser::new(40, 120, 0);
    busy.process(
        frame("Ordinary graph version 09999 • author Writer")
            .contents_formatted()
            .as_slice(),
    );
    busy.process(b"\x1b[20;1H[Esc Cancel]");
    assert!(
        selection::first_page_down_batch(busy.screen(), "Ordinary graph version 09800", SHA)
            .is_err()
    );
    assert!(selection::first_page_down_batch(
        &frame("Ordinary graph version 09999 • author Writer"),
        "Ordinary graph version 09800",
        &"f".repeat(40)
    )
    .is_err());
    assert!(selection::first_page_down_batch(
        &frame("Ordinary graph version 09999 • author Writer"),
        "Ordinary graph version 09800",
        "12345678"
    )
    .is_err());
    let mut longer = vt100::Parser::new(40, 120, 0);
    longer.process(format!("Viewed: {SHA}{}\r\n[Home First loaded]\r\n│\x1b[7mOrdinary graph version 09999 • actor\x1b[0m\r\n", "a".repeat(24)).as_bytes());
    assert!(
        selection::first_page_down_batch(longer.screen(), "Ordinary graph version 09800", SHA)
            .is_err(),
        "full original SHA must not be a prefix match"
    );
}
