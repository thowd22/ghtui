//! Protocol-shaped observed timeline data, not an identity/closure receipt.
use serde_json::{json, Value};

pub fn forbid_http(fixture: &crate::support::Fixture) {
    let program = fixture.root.join("bin/curl");
    assert!(
        !program.exists(),
        "New disabled-image fixture must not overwrite a producer"
    );
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}/timeline-unexpected-http'\nexit 94\n",
        fixture.root.display()
    );
    crate::fixture_shell_publication_v1::install(&program, script.as_bytes()).unwrap();
}
pub fn assert_http_zero(fixture: &crate::support::Fixture) {
    assert!(
        !fixture.root.join("timeline-unexpected-http").exists(),
        "Disabled observed timeline actors cannot dispatch even one public HTTP child"
    );
    assert!(!fixture.root.join("config/ghtui/avatars").exists());
}

fn actor(id: u64, login: &str, kind: &str, image: bool) -> Value {
    let mut actor =
        json!({"id":id,"node_id":format!("TIMELINE_ACTOR_{id}"),"login":login,"type":kind});
    if image {
        actor["avatar_url"] = json!(format!("https://avatars.githubusercontent.com/u/{id}?v=4"));
    }
    actor
}
pub fn events() -> Value {
    json!([
        {"event":"commented","user":actor(202,"comment-writer","User",true),"actor":actor(303,"event-generator[bot]","Bot",true),"created_at":"2026-10-14T01:02:03Z","body":"Exact timeline comment body"},
        {"event":"reviewed","user":actor(202,"comment-writer","User",true),"submitted_at":"2026-10-14T04:05:06Z","created_at":"WRONG_CREATED_TIME","state":"approved","body":"Exact submitted review body"},
        {"event":"commented","user":null,"actor":actor(303,"event-generator[bot]","Bot",true),"body":"Missing author stays unknown"},
        {"event":"commented","user":actor(404,"ghost","User",false),"body":"Deleted comment author remains deleted"},
        {"event":"future-event","actor":actor(303,"event-generator[bot]","Bot",true),"user":{"login":"NOT_PROVEN_FUTURE_AUTHOR"},"body":"Unknown future event observed honestly","label":{"name":"observed-label"}}
    ])
}
pub const EXPECTED: &[&str] = &[
    "comment author",
    "event generator",
    "@comment-writer",
    "@event-generator[bot] [bot]",
    "2026-10-14T01:02:03Z",
    "review author",
    "2026-10-14T04:05:06Z",
    "Exact timeline comment body",
    "Missing author stays unknown",
    "[unknown]",
    "@ghost [deleted]",
    "future-event",
    "observed-label",
];
pub const ABSENT: &[&str] = &["WRONG_CREATED_TIME", "NOT_PROVEN_FUTURE_AUTHOR"];
