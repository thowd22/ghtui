//! Transparent held REAL Git output, current-owner Close/reopen, actual raw
//! staleness. No canned Git response, reduced effects oracle or live remote.
use super::*;
fn shell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn observer(native: &Native) {
    let actual = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(actual.status.success());
    let actual = String::from_utf8(actual.stdout).unwrap();
    let actual = actual.trim();
    assert!(Path::new(actual).is_absolute());
    let root = shell(native.fixture.root.to_str().unwrap());
    let git = shell(actual);
    let program = native.fixture.root.join("bin/git");
    fs::write(&program,format!("#!/bin/sh\nroot={root}\ncase \" $* \" in\n *' cat-file blob '*)\n if [ ! -f \"$root/full-held-entered\" ]; then\n {git} \"$@\" > \"$root/full-held-output\"; result=$?\n printf '%s\\n' \"$result\" > \"$root/full-held-result\"\n printf '%s\\0' \"$@\" > \"$root/full-held-argv\"\n /bin/sleep 30 & child=$!\n printf '%s\\n' \"$child\" > \"$root/full-held-descendant\"\n printf '%s\\n' \"$$\" > \"$root/full-held-pid\"\n printf 'actual installed Git has completed\\n' > \"$root/full-held-entered\"\n while [ ! -f \"$root/full-held-release\" ]; do /bin/sleep 0.02; done\n kill \"$child\" 2>/dev/null; wait \"$child\" 2>/dev/null\n /bin/cat \"$root/full-held-output\"; exit \"$result\"\n fi;;\nesac\nexec {git} \"$@\"\n")).unwrap();
    fs::set_permissions(program, fs::Permissions::from_mode(0o755)).unwrap();
}
fn until(session: &mut Session, mut predicate: impl FnMut() -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if predicate() {
            return;
        }
        assert!(Instant::now() < deadline, "{label}");
    }
}
fn pid(path: &Path) -> i32 {
    let data = fs::read_to_string(path).unwrap();
    assert!(data.ends_with('\n'));
    let pid: i32 = data.trim().parse().unwrap();
    assert!(pid > 0);
    pid
}
fn gone(session: &mut Session, expected: &ProcessIdentity) {
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    until(
        session,
        || {
            let Some(current) = process_identity(expected.pid).unwrap() else {
                return true;
            };
            assert!(owner.owns(&current));
            assert_eq!(
                (current.pid, current.uid, current.session, current.birth),
                (expected.pid, expected.uid, expected.session, expected.birth)
            );
            if current.zombie && current.parent == std::process::id() as i32 {
                let mut status = 0;
                assert_eq!(
                    unsafe { libc::waitpid(current.pid, &mut status, libc::WNOHANG) },
                    current.pid
                );
                assert!(libc::WIFEXITED(status) || libc::WIFSIGNALED(status));
                assert!(process_identity(current.pid).unwrap().is_none());
                true
            } else {
                false
            }
        },
        "exact originally owned full-side child actually gone/reaped",
    );
}
#[test]
fn native_full_side_keyboard_actual_held_read_close_reopen_and_stale_raw_no_publish() {
    cancel(false);
    stale(false);
}
#[test]
fn native_full_side_mouse_actual_held_read_close_reopen_and_stale_raw_no_publish() {
    cancel(true);
    stale(true);
}
fn cancel(mouse: bool) {
    let native = fixture("sha256");
    let before = independent(&native);
    observer(&native);
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    session.choose_compact(mouse, "[v Native diff]", "[v Diff]");
    patch(&mut session, SAVED, "uppercase actual target");
    show(&mut session, mouse, "New");
    until(
        &mut session,
        || native.fixture.root.join("full-held-entered").exists(),
        "actual installed Git blob completed before held delivery",
    );
    assert_eq!(
        fs::read(native.fixture.root.join("full-held-result")).unwrap(),
        b"0\n"
    );
    assert_eq!(
        fs::read(native.fixture.root.join("full-held-output")).unwrap(),
        b"uppercase actual target\n"
    );
    let object = String::from_utf8(git_bytes(
        &native.fixture,
        &["rev-parse", &format!("{}:Case", native.b)],
        None,
    ))
    .unwrap();
    let argv = fs::read(native.fixture.root.join("full-held-argv")).unwrap();
    let fields = argv.split(|byte| *byte == 0).collect::<Vec<_>>();
    assert!(fields
        .windows(3)
        .any(|args| args == [b"cat-file".as_slice(), b"blob", object.trim().as_bytes()]));
    let pair = (
        process_identity(pid(&native.fixture.root.join("full-held-pid")))
            .unwrap()
            .unwrap(),
        process_identity(pid(&native.fixture.root.join("full-held-descendant")))
            .unwrap()
            .unwrap(),
    );
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    assert!(owner.owns(&pair.0) && owner.owns(&pair.1));
    assert_eq!(pair.1.parent, pair.0.pid);
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("Preparing current full-file context")
                && frame.contents().contains("[Esc Close]")
        },
        "owned pending full-side read",
        Duration::from_secs(8),
    );
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains("Native saved diff closed")
                && !frame.contents().contains(SAVED)
        },
        "logical Close retires held full-side",
        Duration::from_secs(8),
    );
    gone(&mut session, &pair.0);
    gone(&mut session, &pair.1);
    session.choose_compact(mouse, "[v Native diff]", "[v Diff]");
    patch(&mut session, SAVED, "uppercase actual target");
    show(&mut session, mouse, "New");
    full(&mut session, "New", "uppercase actual target");
    effects(&native, &session, &before);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native saved diff closed");
    effects(&native, &session, &before);
    session.quit_and_assert_restored_terminal(mouse);
}
fn stale(mouse: bool) {
    let native = fixture("sha1");
    let mut session = launch(&native, mouse);
    session.wait_for_screen("Recent commits");
    choose(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_cursorless_condition(
        |frame| {
            (frame.contents().contains("v Native diff") || frame.contents().contains("v Diff  "))
                && frame.contents().contains("user dirty bytes")
        },
        "actual Working comparison",
        Duration::from_secs(8),
    );
    session.choose_compact(mouse, "[v Native diff]", "[v Diff]");
    patch(&mut session, LOCAL, "user dirty bytes");
    show(&mut session, mouse, "New");
    full(&mut session, "New", "user dirty bytes");
    fs::write(
        native.fixture.repo().join("tracked"),
        b"external independently edited newer bytes\n",
    )
    .unwrap();
    let after_external = independent(&native);
    choose(&mut session, mouse, "[o Options]", b"o");
    choose(&mut session, mouse, "[r Exact / regex]", b"r");
    session.wait_for_cursorless_condition(
        |frame| frame.contents().contains("stale") && frame.contents().contains("[o Back to file]"),
        "fresh actual fingerprint rejects old raw projection",
        Duration::from_secs(8),
    );
    choose(&mut session, mouse, "[o Back to file]", b"o");
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains("FULL New file")
                && screen.contains("stale")
                && !screen.contains("Full-file context ready")
                && !screen.lines().any(|line| line.contains("user dirty bytes"))
        },
        "retired raw rows not painted or clickable after actual external change",
        Duration::from_secs(8),
    );
    effects(&native, &session, &after_external);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native local diff closed");
    effects(&native, &session, &after_external);
    assert_eq!(
        fs::read(native.fixture.repo().join("tracked")).unwrap(),
        b"external independently edited newer bytes\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}
