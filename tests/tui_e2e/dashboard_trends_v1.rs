//! TASK-61/62 retained metric history on the actual executable: a Dashboard load
//! retains one point per card and UTC day, the Trend widget labels missing and
//! partial days, and a retained day drills down to its record and actor IDs.
use super::*;
use rusqlite::{params, Connection};

const KEY: &str = "example/app|7d";

fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/cache.sqlite3")
}
fn date(day: u64) -> String {
    utc(day * 86_400)[..10].to_owned()
}
fn seeded(day: u64) -> Value {
    json!({
        "version": 1,
        "observed_at": day * 86_400 + 3_600,
        "value": 4,
        "value_label": "4",
        "coverage": {"Partial": ["seeded tools inventory missing"]},
        "unknown": 1,
        "censored": 0,
        "contributors": [{
            "item": "example/app#7",
            "ids": "repository id 410 • PR id 1007 • node PR_seeded_7",
            "actors": ["bob (id 7)"],
            "measure": "current record"
        }],
        "omitted": 0
    })
}
fn seed(fixture: &Fixture, account: u64, day: u64) {
    Cache::open(&database(fixture))
        .unwrap()
        .retain_metric_trend_points_v1(
            &Scope::github(gh_fixture::HOST, Some(account)).unwrap(),
            KEY,
            day,
            &[("OpenPrs".into(), seeded(day))],
        )
        .unwrap();
}
fn points(fixture: &Fixture, account: u64) -> Vec<(String, u64, Value)> {
    let sql = Connection::open(database(fixture)).unwrap();
    let scope = Scope::github(gh_fixture::HOST, Some(account)).unwrap();
    let mut statement = sql
        .prepare(
            "SELECT card,day,payload FROM metric_trend_points_v1 WHERE host=?1 AND login=?2 AND scope_key=?3 ORDER BY day,card",
        )
        .unwrap();
    statement
        .query_map(params![scope.host, scope.login, KEY], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                serde_json::from_str(&row.get::<_, String>(2)?).unwrap(),
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn dashboard_trends_keyboard_retains_history_labels_gaps_and_drills_into_record_ids() {
    trends(false);
}
#[test]
fn dashboard_trends_mouse_retains_history_labels_gaps_and_drills_into_record_ids() {
    trends(true);
}
fn trends(mouse: bool) {
    let fixture = configured(false);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let today = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / 86_400;
    seed(&fixture, 42, today - 2);
    // Another account's retained day never appears in account 42's trend.
    seed(&fixture, 43, today - 1);
    let steps = refresh(&[APP]);
    assert_eq!(steps.len(), 7);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 160, 42);
    start(&mut session, mouse);
    journal(&mut session, &fixture, 7);
    cards_ready(&mut session, 1, 1);
    if mouse {
        // From a card's contributors, the Trend control opens that card's trend.
        session.click_visible_action(CARDS[0]);
        session.wait_for_screen("example/app#1");
        session.click_visible_action("[t Trend]");
    } else {
        session.send_key(b"\x1b[H");
        selected(&mut session, CARDS[0]);
        session.send_key(b"t");
    }
    session.wait_for_screen("Trend value: record count");
    session.wait_for_screen("Retained 2 of 7 UTC days (1 partial)");
    session.wait_for_screen("days before retention began are not backfilled");
    session.wait_for_screen(&format!(
        "{}  Missing: no retained observation (not zero)",
        date(today - 1)
    ));
    session.wait_for_screen(&format!(
        "{}  4  • Partial: seeded tools inventory missing • 1 records",
        date(today - 2)
    ));
    let current = format!("{}  1  •", date(today));
    session.wait_for_screen(&current);

    // The partial retained day drills into its retained record and actor IDs.
    if mouse {
        session.click_visible_action("Partial: seeded tools inventory missing");
    } else {
        session.send_key(b"\x1b[F");
        selected(&mut session, &current);
        session.send_key(b"\x1b[A");
        session.send_key(b"\x1b[A");
        selected(&mut session, "Partial: seeded tools inventory missing");
        session.send_key(b"\r");
    }
    session.wait_for_screen(&format!("{} retained observation", date(today - 2)));
    session.wait_for_screen("example/app#7 • current record");
    session.wait_for_screen("IDs: repository id 410 • PR id 1007 • node PR_seeded_7");
    session.wait_for_screen("Actors: bob (id 7)");
    back(&mut session, mouse);
    session.wait_for_screen("Retained 2 of 7 UTC days (1 partial)");

    // Today's point was retained by this load, with the exact contributing record.
    if mouse {
        session.click_visible_action(&current);
    } else {
        session.send_key(b"\x1b[F");
        selected(&mut session, &current);
        session.send_key(b"\r");
    }
    session.wait_for_screen(&format!("{} retained observation", date(today)));
    session.wait_for_screen("example/app#1 • current record");
    session.wait_for_screen("IDs: repository id 410 • PR id 1001 • node PR_dashboard_1");
    session.wait_for_screen("Actors: alice (id 51)");
    session.resize(80, 24);
    session.wait_for_screen("Actors: alice (id 51)");
    back(&mut session, mouse);
    session.wait_for_screen("Retained 2 of 7 UTC days (1 partial)");
    back(&mut session, mouse);
    if mouse {
        session.wait_for_screen("example/app#1");
    } else {
        cards_ready(&mut session, 1, 1);
    }
    // No trend read reaches GitHub.
    journal(&mut session, &fixture, 7);
    session.quit_and_assert_restored_terminal(mouse);
    journal(&mut session, &fixture, 7);

    let retained = points(&fixture, 42);
    assert_eq!(
        retained.len(),
        9,
        "the seeded day plus one point per card today"
    );
    assert_eq!(
        retained[0],
        ("OpenPrs".into(), today - 2, seeded(today - 2))
    );
    let cards = retained[1..]
        .iter()
        .map(|(card, day, _)| {
            assert_eq!(*day, today);
            card.as_str()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cards,
        [
            "Checks",
            "FirstReview",
            "MergeLatency",
            "NeedsMyReview",
            "OpenAge",
            "OpenPrs",
            "ReviewWait",
            "Workflows"
        ]
    );
    let open = &retained
        .iter()
        .find(|(card, day, _)| card == "OpenPrs" && *day == today)
        .unwrap()
        .2;
    assert_eq!(open["value"], 1);
    assert_eq!(open["contributors"][0]["item"], "example/app#1");
    assert_eq!(
        open["contributors"][0]["ids"],
        "repository id 410 • PR id 1001 • node PR_dashboard_1"
    );
    assert_eq!(open["contributors"][0]["actors"], json!(["alice (id 51)"]));
    assert_eq!(open["observed_at"].as_u64().unwrap() / 86_400, today);
    let foreign = points(&fixture, 43);
    assert_eq!(foreign.len(), 1, "account 43 keeps only its own seeded day");
    assert_eq!(foreign[0].2, seeded(today - 1));
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
}
