//! SOURCE-ONLY actual-binary declarations. Root owns all executions/results.
use super::*;
use std::io::{Cursor, Write};
fn artifact() -> Value {
    json!({"id":701,"node_id":"ART_701","name":"ACTUAL_PRIVATE_BUILD","size_in_bytes":128,"expired":false,"created_at":"2026-10-15T00:00:00Z","updated_at":"2026-10-15T00:00:00Z","expires_at":"2026-10-22T00:00:00Z","workflow_run":{"id":123,"repository_id":10000,"head_sha":H}})
}
fn workflow() -> Value {
    json!({"id":7,"node_id":"WF_7","name":"ACTUAL_PRIVATE_WORKFLOW","path":".github/workflows/ci.yml","state":"active"})
}
pub(super) fn resource(body: Vec<GhStep>, uid: u64) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(body);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(uid));
    out
}
fn catalogue(page: u32, status: Option<&str>, total: u64, id: u64) -> Vec<GhStep> {
    let filter = status.map_or(String::new(), |s| format!("&status={s}"));
    let mut raw = fresh_run(CURRENT);
    raw["id"] = json!(id);
    raw["node_id"] = json!(format!("WR_{id}"));
    resource(
        vec![get(
            &format!("repos/{CURRENT}/actions/runs?per_page=100{filter}&page={page}"),
            json!({"total_count":total,"workflow_runs":[raw]}),
        )],
        42,
    )
}
fn artifacts() -> Vec<GhStep> {
    resource(
        vec![
            get(
                &format!("repos/{CURRENT}/actions/runs/123"),
                fresh_run(CURRENT),
            ),
            paged(
                &format!("repos/{CURRENT}/actions/runs/123/artifacts?per_page=100"),
                json!({"total_count":1,"artifacts":[artifact()]}),
            ),
            get(
                &format!("repos/{CURRENT}/actions/runs/123"),
                fresh_run(CURRENT),
            ),
        ],
        42,
    )
}
fn workflows(uid: u64) -> Vec<GhStep> {
    resource(
        vec![paged(
            &format!("repos/{CURRENT}/actions/workflows?per_page=100"),
            json!({"total_count":1,"workflows":[workflow()]}),
        )],
        uid,
    )
}
fn settings_steps(which: usize) -> Vec<GhStep> {
    let (endpoint, key, rows) = match which {
        0 => (
            format!("repos/{CURRENT}/actions/secrets?per_page=100"),
            Some("secrets"),
            json!({"secrets":[{"name":"SCOPED_SECRET_NAME_ONLY","created_at":"created","updated_at":"updated"}]}),
        ),
        1 => (
            format!("repos/{CURRENT}/actions/variables?per_page=100"),
            Some("variables"),
            json!({"variables":[{"name":"SCOPED_VARIABLE","value":"PRIVATE_ACTUAL_VARIABLE","created_at":"created","updated_at":"updated"}]}),
        ),
        2 => (
            format!("repos/{CURRENT}/actions/permissions"),
            None,
            json!({"enabled":true,"allowed_actions":"selected"}),
        ),
        3 => (
            format!("repos/{CURRENT}/environments?per_page=100"),
            Some("environments"),
            json!({"environments":[{"id":801,"name":"ACTUAL_PRODUCTION_ENV","protection_rules":[]}]}),
        ),
        4 => (
            format!("repos/{CURRENT}/actions/runners?per_page=100"),
            Some("runners"),
            json!({"runners":[{"id":901,"name":"ACTUAL_LINUX_RUNNER","os":"linux","status":"online","busy":false,"labels":[{"id":1,"name":"self-hosted","type":"read-only"}]}]}),
        ),
        _ => (
            format!("repos/{CURRENT}/actions/caches?per_page=100"),
            Some("actions_caches"),
            json!({"actions_caches":[{"id":1001,"key":"ACTUAL_CACHE_KEY","ref":"refs/heads/main","version":"v1","size_in_bytes":123,"created_at":"created","last_accessed_at":"used"}]}),
        ),
    };
    let mut body = vec![if key.is_some() {
        paged(&endpoint, rows)
    } else {
        get(&endpoint, rows)
    }];
    if which == 5 {
        body.push(get(
            &format!("repos/{CURRENT}/actions/cache/usage"),
            json!({"active_caches_count":1,"active_caches_size_in_bytes":123}),
        ));
    }
    resource(body, 42)
}
fn settings(mouse: bool) {
    let mut steps = actions_protocol();
    for which in 0..6 {
        steps.extend(settings_steps(which));
    }
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    let mut count = 14;
    for (which, label, key, target) in [
        (0, "Secrets", b"S".as_slice(), "SCOPED_SECRET_NAME_ONLY"),
        (1, "Variables", b"v".as_slice(), "PRIVATE_ACTUAL_VARIABLE"),
        (2, "Policies", b"p".as_slice(), "selected"),
        (3, "Environments", b"e".as_slice(), "ACTUAL_PRODUCTION_ENV"),
        (4, "Runners", b"n".as_slice(), "ACTUAL_LINUX_RUNNER"),
        (5, "Caches", b"c".as_slice(), "ACTUAL_CACHE_KEY"),
    ] {
        let after = session.completed_cursorless_end();
        action(&mut session, mouse, label, key);
        count += if which == 5 { 8 } else { 7 };
        session.wait_for_command_count(&fixture, count);
        current_body(&mut session, after, &[target]);
        if which == 0 {
            assert!(!session.screen().contains("PRIVATE_ACTUAL_VARIABLE"));
        }
        assert_eq!(commands(&session, &fixture).len(), count);
        session.resize(
            if which % 2 == 0 { 80 } else { 120 },
            if which % 2 == 0 { 24 } else { 40 },
        );
        session.wait_for_main_frame();
    }
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        width,
        height,
        image::Rgba([40, 90, 170, 255]),
    ))
    .write_to(&mut out, image::ImageFormat::Png)
    .unwrap();
    out.into_inner()
}
fn images(mouse: bool) {
    for too_wide in [false, true] {
        let bytes = png(if too_wide { 1025 } else { 8 }, 8);
        let mut steps = actions_protocol();
        steps.extend(operation(ReadKind::Run, 42, CURRENT));
        steps.extend(artifacts());
        steps.extend(archive(zip("actual.png", &bytes), true));
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        let sql = sql(&fixture);
        open_run(&mut session, &fixture, mouse, 24, CURRENT);
        let after = session.completed_cursorless_end();
        action(&mut session, mouse, "[a Artifacts]", b"a");
        session.wait_for_command_count(&fixture, 33);
        current_body(&mut session, after, &["ACTUAL_PRIVATE_BUILD"]);
        let after = session.completed_cursorless_end();
        selected_row(&mut session, mouse, "ACTUAL_PRIVATE_BUILD");
        session.wait_for_command_count(&fixture, 45);
        current_body(&mut session, after, &["actual.png"]);
        let after = session.completed_cursorless_end();
        selected_row(&mut session, mouse, "actual.png");
        current_body(
            &mut session,
            after,
            &[if too_wide {
                "Image decode unavailable"
            } else {
                "Bounded PNG image"
            }],
        );
        if !too_wide {
            assert!(session.screen().contains("8×8 original pixels"));
        }
        assert_eq!(commands(&session, &fixture).len(), 45);
        session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
        session.wait_for_main_frame();
        end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
    }
}
fn zip(path: &str, bytes: &[u8]) -> Vec<u8> {
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    out.start_file(
        path,
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    out.write_all(bytes).unwrap();
    out.finish().unwrap().into_inner()
}
fn archive(bytes: Vec<u8>, valid: bool) -> Vec<GhStep> {
    let mut raw = GhStep::read(
        &[
            "api",
            &format!("repos/{CURRENT}/actions/artifacts/701/zip"),
            "--hostname",
            HOST,
            "--method",
            "GET",
            "--allow-escape-sequences",
        ],
        "",
    );
    raw.stdout = bytes;
    let mut body = vec![
        get(
            &format!("repos/{CURRENT}/actions/runs/123"),
            fresh_run(CURRENT),
        ),
        get(
            &format!("repos/{CURRENT}/actions/artifacts/701"),
            artifact(),
        ),
        get(
            &format!("repos/{CURRENT}/actions/artifacts/701"),
            artifact(),
        ),
        raw,
    ];
    if valid {
        body.push(get(
            &format!("repos/{CURRENT}/actions/artifacts/701"),
            artifact(),
        ));
        body.push(get(
            &format!("repos/{CURRENT}/actions/runs/123"),
            fresh_run(CURRENT),
        ));
    }
    resource(body, 42)
}
fn define(closing: Value) -> Vec<GhStep> {
    let sha = "c".repeat(40);
    resource(vec![get(&format!("repos/{CURRENT}/actions/workflows/7"),workflow()),get(&format!("repos/{CURRENT}/commits/main"),json!({"sha":sha})),GhStep::read(&["api",&format!("repos/{CURRENT}/contents/.github/workflows/ci.yml?ref={sha}"),"--hostname",HOST,"--method","GET","--header","Accept: application/vnd.github.raw+json","--allow-escape-sequences"],"name: ACTUAL_PRIVATE_WORKFLOW\non: [push]\njobs:\n  actual_job:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_EXECUTED_WORKFLOW\n"),get(&format!("repos/{CURRENT}/actions/workflows/7"),closing)],42)
}
pub(super) fn action(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if !mouse {
        session.send_key(key);
        return;
    }
    // Catalogue tabs, key-bar tokens, or the palette when the bar cannot fit the control.
    session.click_actions_control(label);
}
fn current_body(session: &mut Session, after: usize, values: &[&str]) {
    complete(session, after, values);
    assert!(!session
        .screen()
        .contains("PRIVATE_SCOPED_ACTIONS_FAILURE_DIAGNOSTIC"));
}
fn pages(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(catalogue(1, None, 10000, 123));
    steps.extend(catalogue(2, None, 10000, 124));
    steps.extend(catalogue(1, Some("failure"), 300, 123));
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_command_count(&fixture, 21);
    current_body(
        &mut session,
        after,
        &[
            "Run page 1",
            "/10000 reported",
            "partial is not absence",
            "123 #2",
        ],
    );
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "[) Next runs]", b")");
    session.wait_for_command_count(&fixture, 28);
    current_body(&mut session, after, &["Run page 2", "124 #2"]);
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "[F Failed runs]", b"F");
    session.wait_for_command_count(&fixture, 35);
    current_body(
        &mut session,
        after,
        &["status failure", "/300 reported", "123 #2"],
    );
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    session.wait_for_main_frame();
    open_run(&mut session, &fixture, mouse, 45, CURRENT);
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn definition(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(workflows(42));
    steps.extend(define(workflow()));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "Workflows", b"W");
    session.wait_for_command_count(&fixture, 21);
    current_body(
        &mut session,
        after,
        &["ACTUAL_PRIVATE_WORKFLOW", ".github/workflows/ci.yml"],
    );
    let after = session.completed_cursorless_end();
    selected_row(&mut session, mouse, "ACTUAL_PRIVATE_WORKFLOW");
    session.wait_for_command_count(&fixture, 31);
    current_body(
        &mut session,
        after,
        &[
            "Requested ref main",
            "cccccccccccccccccccccccccccccccccccccccc",
        ],
    );
    for _ in 0..8 {
        if session.screen().contains("NEVER_EXECUTED_WORKFLOW") {
            break;
        }
        session.send_key(b"\x1b[6~");
        session.wait_for_main_frame();
    }
    assert!(session.screen().contains("NEVER_EXECUTED_WORKFLOW"));
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    session.wait_for_main_frame();
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "[b Back]", b"b");
    current_body(
        &mut session,
        after,
        &["ACTUAL_PRIVATE_WORKFLOW", ".github/workflows/ci.yml"],
    );
    assert_eq!(commands(&session, &fixture).len(), 31);
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn preview(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(artifacts());
    steps.extend(archive(
        zip("nested/界.txt", b"ACTUAL_PRIVATE_ARCHIVE\nsecond line\n"),
        true,
    ));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    open_run(&mut session, &fixture, mouse, 24, CURRENT);
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "[a Artifacts]", b"a");
    session.wait_for_command_count(&fixture, 33);
    current_body(
        &mut session,
        after,
        &["ACTUAL_PRIVATE_BUILD", "attempt not exposed"],
    );
    let after = session.completed_cursorless_end();
    selected_row(&mut session, mouse, "ACTUAL_PRIVATE_BUILD");
    session.wait_for_command_count(&fixture, 45);
    current_body(
        &mut session,
        after,
        &["nested/界.txt", "Artifact ACTUAL_PRIVATE_BUILD"],
    );
    let after = session.completed_cursorless_end();
    selected_row(&mut session, mouse, "nested/界.txt");
    current_body(
        &mut session,
        after,
        &["ACTUAL_PRIVATE_ARCHIVE", "complete file"],
    );
    assert_eq!(commands(&session, &fixture).len(), 45);
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    session.wait_for_main_frame();
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "[b Back]", b"b");
    current_body(
        &mut session,
        after,
        &["nested/界.txt", "Artifact ACTUAL_PRIVATE_BUILD"],
    );
    assert_eq!(commands(&session, &fixture).len(), 45);
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn private_loss(mouse: bool) {
    let mut steps = actions_protocol();
    steps.extend(workflows(43));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "Workflows", b"W");
    session.wait_for_command_count(&fixture, 21);
    current_body(
        &mut session,
        after,
        &["Original Actions read unavailable", "private source hidden"],
    );
    assert!(!session.screen().contains("ACTUAL_PRIVATE_WORKFLOW"));
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn held_cancel(mouse: bool) {
    let mut held = workflows(42);
    held[3].delay_ms = 60000;
    held.truncate(4);
    let mut steps = actions_protocol();
    steps.extend(held);
    steps.extend(workflows(42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    let sql = sql(&fixture);
    action(&mut session, mouse, "Workflows", b"W");
    let child = entered(&mut session, &fixture, 18);
    session.wait_for_cursorless_condition(
        |frame| crate::support::locate_control(frame, "[b Cancel read]").is_some(),
        "Actual pending resource Cancel",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "[b Cancel read]", b"b");
    current_body(&mut session, after, &["read canceled", "123 #2"]);
    gone(&mut session, &child);
    assert_eq!(commands(&session, &fixture).len(), 18);
    let after = session.completed_cursorless_end();
    action(&mut session, mouse, "Workflows", b"W");
    session.wait_for_command_count(&fixture, 25);
    current_body(&mut session, after, &["ACTUAL_PRIVATE_WORKFLOW"]);
    end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
}
fn hostile(mouse: bool) {
    for case in 0..4 {
        let mut bytes = zip(
            if case == 0 { "../escape" } else { "real.txt" },
            b"CRC_ORIGINAL",
        );
        if case == 1 {
            let at = bytes.windows(4).rposition(|w| w == b"PK\x05\x06").unwrap();
            bytes[at + 8..at + 12].copy_from_slice(&[0xff; 4]);
        }
        if case == 2 {
            bytes = vec![b'X'; 4 * 1024 * 1024 + 1];
        }
        if case == 3 {
            let at = bytes
                .windows(12)
                .position(|w| w == b"CRC_ORIGINAL")
                .unwrap();
            bytes[at] = b'X';
        }
        let mut steps = actions_protocol();
        steps.extend(operation(ReadKind::Run, 42, CURRENT));
        steps.extend(artifacts());
        steps.extend(archive(bytes, case == 3));
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        let sql = sql(&fixture);
        open_run(&mut session, &fixture, mouse, 24, CURRENT);
        let after = session.completed_cursorless_end();
        action(&mut session, mouse, "[a Artifacts]", b"a");
        session.wait_for_command_count(&fixture, 33);
        current_body(&mut session, after, &["ACTUAL_PRIVATE_BUILD"]);
        let after = session.completed_cursorless_end();
        selected_row(&mut session, mouse, "ACTUAL_PRIVATE_BUILD");
        session.wait_for_command_count(&fixture, steps.len());
        if case == 3 {
            current_body(&mut session, after, &["real.txt"]);
            let after = session.completed_cursorless_end();
            selected_row(&mut session, mouse, "real.txt");
            current_body(
                &mut session,
                after,
                &["Original Actions read unavailable", "private source hidden"],
            );
        } else {
            current_body(
                &mut session,
                after,
                &["Original Actions read unavailable", "private source hidden"],
            );
        }
        assert!(!session.screen().contains("CRC_ORIGINAL"));
        assert!(!fixture.root.join("escape").exists());
        end(&mut session, &fixture, mouse, &steps, &git, &config, &sql);
    }
}
#[test]
fn scoped_actions_resources_v2_keyboard_partial_real_run_pages_filter_and_actual_member() {
    pages(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_partial_real_run_pages_filter_and_actual_member() {
    pages(true);
}
#[test]
fn scoped_actions_resources_v2_keyboard_workflow_requested_ref_actual_sha_yaml_back_resize() {
    definition(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_workflow_requested_ref_actual_sha_yaml_back_resize() {
    definition(true);
}
#[test]
fn scoped_actions_resources_v2_keyboard_artifact_original_zip_entry_preview_back_zero_effects() {
    preview(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_artifact_original_zip_entry_preview_back_zero_effects() {
    preview(true);
}
#[test]
fn scoped_actions_resources_v2_keyboard_changed_numeric_uid_hides_prepared_private_resource() {
    private_loss(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_changed_numeric_uid_hides_prepared_private_resource() {
    private_loss(true);
}
#[test]
fn scoped_actions_resources_v2_keyboard_held_resource_cancel_exact_child_gone_then_fresh_read() {
    held_cancel(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_held_resource_cancel_exact_child_gone_then_fresh_read() {
    held_cancel(true);
}
#[test]
fn scoped_actions_resources_v2_keyboard_traversal_count_overflow_output_bound_crc_refusal() {
    hostile(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_traversal_count_overflow_output_bound_crc_refusal() {
    hostile(true);
}
#[test]
fn scoped_actions_resources_v2_keyboard_settings_read_names_values_policies_environments_runners_caches(
) {
    settings(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_settings_read_names_values_policies_environments_runners_caches(
) {
    settings(true);
}
#[test]
fn scoped_actions_resources_v2_keyboard_real_png_bounded_image_and_decoded_dimension_refusal() {
    images(false);
}
#[test]
fn scoped_actions_resources_v2_mouse_real_png_bounded_image_and_decoded_dimension_refusal() {
    images(true);
}
