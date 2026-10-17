//! Real Main ordinary Save ACK AFTER an actual selected-tip refusal. The old
//! uncontrolled refusal pair is unchanged; these sessions force the late ACK.
use super::*;
use rusqlite::types::Value as Sql;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
const REFUSAL: &str = "Original Conversation unavailable • nothing resent • draft/journal retained";
const SAVED: &str = "Draft saved locally • nothing published";
const BLANK: &str = "{\"version\":17,\"values\":{\"base\":\"\",\"tip\":\"\"},\"publication\":null}";
type SchemaV314 = Vec<(String, String, String)>;
type RowsV314 = Vec<Vec<Sql>>;
#[derive(Clone, Debug, PartialEq)]
struct FullSqlV314 {
    schema: SchemaV314,
    tables: BTreeMap<String, RowsV314>,
}
fn full_sql(f: &Fixture) -> FullSqlV314 {
    let db = raw(f);
    let schema = db
        .prepare("SELECT type,name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut tables = BTreeMap::new();
    for (kind, name, _) in &schema {
        if kind != "table" {
            continue;
        }
        let mut st = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let n = st.column_count();
        let mut rows = st
            .query_map([], |r| {
                (0..n)
                    .map(|i| r.get::<_, Sql>(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort_by_key(|r| format!("{r:?}"));
        tables.insert(name.clone(), rows);
    }
    FullSqlV314 { schema, tables }
}
fn draft_key() -> String {
    // Independent literal original tuple, not a production key serializer.
    format!(
        "metadata-base-v32/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                json!({"host":HOST,"login":"user-id:42"}),
                10000u64,
                "REPO_catalogue_10000",
                1007u64,
                "GENUINE_CANONICAL_PR_NODE_1007",
                7u64,
                "a".repeat(40),
                "b".repeat(40),
                "main"
            ))
            .unwrap()
        )
    )
}
fn draft(f: &Fixture, key: &str) -> Option<(String, i64)> {
    raw(f).query_row("SELECT body,updated_at FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",rusqlite::params![HOST,key],|r|Ok((r.get(0)?,r.get(1)?))).optional().unwrap()
}
fn wall() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}
fn saved_status(frame: &vt100::Screen) -> bool {
    frame
        .contents()
        .lines()
        .nth(usize::from(frame.size().0.saturating_sub(2)))
        .is_some_and(|line| line.trim() == SAVED)
}
#[derive(Clone, Copy)]
enum FrameV314 {
    Selected,
    Refused,
    Saved,
}
fn wait_form(s: &mut Session, after: usize, cols: u16, rows: u16, phase: FrameV314) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let ready = s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                if frame.size() != (rows, cols) || !crate::support::main_footer_visible(frame) {
                    return false;
                }
                let Some(body) = crate::support::overlay_body(frame, &title()) else {
                    return false;
                };
                let controls = body.lines().any(|line| {
                    let mut rest = line;
                    FORM_CONTROLS.iter().all(|label| {
                        if let Some(i) = rest.find(label) {
                            rest = &rest[i + label.len()..];
                            true
                        } else {
                            false
                        }
                    })
                });
                if !controls
                    || !body.contains("Selected existing base branch (F2 Choose)")
                    || !body.contains("Exact observed selected tip (read only)")
                    || !body.contains("next")
                    || !body.contains(TIP)
                    || body.contains("[Esc Cancel]")
                    || body.contains("[Confirm action]")
                    || body.contains("READ-ONLY PREVIEW")
                {
                    return false;
                }
                match phase {
                    FrameV314::Selected => !body.contains(REFUSAL),
                    FrameV314::Refused => {
                        crate::owned_form_prose_v120::adjacent_literal(&body, REFUSAL)
                    }
                    FrameV314::Saved => {
                        saved_status(frame)
                            && crate::owned_form_prose_v120::adjacent_literal(&body, REFUSAL)
                    }
                }
            });
        if ready {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW exact current Base phase/error/controls at{cols}x{rows}\n{}",
            s.screen()
        );
    }
}
fn held_ack(mouse: bool) {
    let mut steps = initial_steps(false);
    let initial = steps.len();
    assert_eq!(initial, 31);
    steps.extend(catalogue(json!(true)));
    let opened = steps.len();
    assert_eq!(opened, 41);
    let mut refused = selected();
    refused[5].stdout = serde_json::to_vec(&branch("next", &"d".repeat(40))).unwrap();
    refused.truncate(6);
    steps.extend(refused);
    assert_eq!(steps.len(), 47);
    let (f, mut s, git, config, stable) = begin(&steps, false, mouse, initial);
    enter_base(&mut s, &f, mouse, opened);
    // Keyboard launch is 120x40, mouse launch is 80x24. Establish the exact
    // required viewport BEFORE holding SQLite; a wrong-size wait must not
    // accidentally turn this ACK-ordering journey into a lock-timeout case.
    if s.completed_cells_screen().unwrap().size() != (24, 80) {
        let resized_after = s.completed_cells_end();
        s.resize(80, 24);
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > resized_after
                && s.completed_cells_screen().is_some_and(|frame| {
                    frame.size() == (24, 80)
                        && crate::support::main_footer_visible(frame)
                        && crate::support::overlay_body(frame, PICKER)
                            .is_some_and(|body| PICKER_CONTROLS.iter().all(|c| body.contains(c)))
                })
            {
                break;
            }
            assert!(Instant::now() < until, "New owned 80x24 picker before lock");
        }
    }
    let key = draft_key();
    // Settle ONLY the initial blank draft/ACK before holding the NEXT selected
    // Save. This is not added to the original uncontrolled refusal journey.
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let committed = draft(&f, &key).is_some_and(|(body, _)| body == BLANK);
        let acknowledged = s.completed_cells_screen().is_some_and(|frame| {
            crate::support::overlay_body(frame, PICKER).is_some() && saved_status(frame)
        });
        if committed && acknowledged {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual initial blank17body and Main ACK never settled\n{}",
            s.screen()
        );
    }
    if mouse {
        s.click_visible_action("Filter focused:");
    }
    s.send_key(b"\x01");
    s.paste("next");
    s.wait_for_screen("Filter focused: next");
    if mouse {
        s.click_visible_action("◆ eligible · next");
    }
    let choose = if mouse {
        Some(s.wait_for_owned_completed_action(PICKER, "[Enter Choose]", &[]))
    } else {
        let notices = [
            format!("{PICKER} · Filter focused"),
            format!("{PICKER} · Rows focused"),
        ];
        let notices = notices.each_ref().map(String::as_str);
        s.focus_completed_owned_action_v1(
            PICKER,
            "[Enter Choose]",
            FocusAttributeV1::Inverse,
            PICKER_CONTROLS,
            &notices,
        );
        None
    };
    let before = full_sql(&f);
    let earliest = wall();
    let mut database = rusqlite::Connection::open_with_flags(
        database(&f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    let held = database
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    // Exact Source audit: this six-role StaleHead route fails in the first
    // validate_selected GET BEFORE prepare()/phase.local()/Cache::open. The
    // lock may block only ordinary writer Save, not invent its provider error.
    let selected_after = s.completed_cells_end();
    if let Some((x, y)) = choose {
        s.click(x, y);
    } else {
        s.send_key(b"\r");
    }
    wait_form(&mut s, selected_after, 80, 24, FrameV314::Selected);
    let refusal_after = s.completed_cells_end();
    control(&mut s, mouse, &title(), "[Preview]", FORM_CONTROLS);
    s.wait_for_command_count(&f, 47);
    wait_form(&mut s, refusal_after, 80, 24, FrameV314::Refused);
    assert_eq!(
        draft(&f, &key).unwrap().0,
        BLANK,
        "Selected Save cannot commit while exact external Immediate is held"
    );
    assert_eq!(
        full_sql(&f),
        before,
        "Real tip refusal before local boundary/claim/save/write is otherwise read-only"
    );
    let ack_after = s.completed_cells_end();
    held.commit().unwrap();
    wait_form(&mut s, ack_after, 80, 24, FrameV314::Saved);
    let selected=format!("{{\"version\":17,\"values\":{{\"base\":\"next\",\"tip\":\"{TIP}\"}},\"publication\":null}}");
    let (actual, stamp) = draft(&f, &key).unwrap();
    assert_eq!(actual, selected);
    assert!((earliest..=wall()).contains(&stamp));
    let mut expected = before.clone();
    let rows = expected.tables.get_mut("drafts").unwrap();
    let own = rows
        .iter_mut()
        .filter(|row| {
            row.first() == Some(&Sql::Text(HOST.into()))
                && row.get(1) == Some(&Sql::Text("user-id:42".into()))
                && row.get(2) == Some(&Sql::Text(key.clone()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        own.len(),
        1,
        "Exactly one independently named original blank Base row"
    );
    let own = own.into_iter().next().unwrap();
    assert_eq!(own.len(), 5);
    assert_eq!(own[3], Sql::Text(BLANK.into()));
    own[3] = Sql::Text(selected);
    own[4] = Sql::Integer(stamp);
    rows.sort_by_key(|row| format!("{row:?}"));
    assert_eq!(full_sql(&f),expected,"Only actual own selected17body/bounded timestamp changes; all schemas/rows/sequences protected");
    s.capture_completed_terminal_v1("base-preview-refusal-retained-after-late-saved-ack");
    for (cols, rows) in [(120, 40), (80, 24)] {
        let after = s.completed_cells_end();
        s.resize(cols, rows);
        wait_form(&mut s, after, cols, rows, FrameV314::Refused);
    }
    assert_eq!(full_sql(&f), expected);
    control(&mut s, mouse, &title(), "[Esc Close]", FORM_CONTROLS);
    s.wait_for_overlay_close(&title());
    assert_eq!(full_sql(&f), expected);
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
#[test]
fn pr_metadata_base_v32_keyboard_actual_late_saved_ack_preserves_owned_tip_refusal() {
    held_ack(false)
}
#[test]
fn pr_metadata_base_v32_mouse_actual_late_saved_ack_preserves_owned_tip_refusal() {
    held_ack(true)
}
