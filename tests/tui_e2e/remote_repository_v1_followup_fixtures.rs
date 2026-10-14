//! Independent real-Git objects/metadata and exact runtime effect oracles.
use super::*;
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn git_input(fixture: &Fixture, args: &[&str], input: &[u8]) -> String {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}
fn tree_json(fixture: &Fixture, tree: &str) -> Value {
    let bytes = fx::git(&fixture.repo(), &["ls-tree", "-z", tree]);
    let rows = bytes
        .split(|byte| *byte == 0)
        .filter(|row| !row.is_empty())
        .map(|row| {
            let tab = row.iter().position(|byte| *byte == b'\t').unwrap();
            let fields = std::str::from_utf8(&row[..tab])
                .unwrap()
                .split(' ')
                .collect::<Vec<_>>();
            let path = std::str::from_utf8(&row[tab + 1..]).unwrap();
            let mut item = json!({"mode":fields[0],"type":fields[1],"sha":fields[2],"path":path});
            if fields[1] == "blob" {
                let size =
                    String::from_utf8(fx::git(&fixture.repo(), &["cat-file", "-s", fields[2]]))
                        .unwrap();
                item["size"] = json!(size.trim().parse::<u64>().unwrap());
            }
            item
        })
        .collect::<Vec<_>>();
    json!({"sha":tree,"truncated":false,"tree":rows})
}
/// Creates actual immutable objects only BEFORE launch. Never changes HEAD,
/// refs, index, config or checked-out files captured by the parent's oracle.
pub(super) fn extended_objects(fixture: &Fixture, objects: &mut fx::Objects) {
    let oversized = git_input(
        fixture,
        &["hash-object", "-w", "--stdin"],
        &vec![b'x'; 1024 * 1024 + 1],
    );
    let mut input = fx::git(&fixture.repo(), &["ls-tree", "-z", &objects.tree]);
    input.extend_from_slice(format!("100644 blob {oversized}\toversized.bin\0").as_bytes());
    input.extend_from_slice(format!("160000 commit {}\tmodule\0", objects.commit).as_bytes());
    let tree = git_input(fixture, &["mktree", "-z"], &input);
    let commit = git_input(
        fixture,
        &["commit-tree", &tree, "-p", &objects.commit],
        b"Independent remote special-content snapshot\n",
    );
    objects.tree = tree;
    objects.commit = commit;
    objects.root_json = tree_json(fixture, &objects.tree);
    assert!(objects.root_json["tree"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["path"] == "module" && row["mode"] == "160000"));
    assert!(objects.root_json["tree"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["path"] == "oversized.bin" && row["size"] == 1024 * 1024 + 1));
    fx::preserved(fixture, objects);
}
pub(super) fn row_index(objects: &fx::Objects, name: &str) -> usize {
    objects.root_json["tree"]
        .as_array()
        .unwrap()
        .iter()
        .position(|row| row["path"] == name)
        .unwrap()
}
pub(super) fn blob_operation(fixture: &Fixture, objects: &fx::Objects, name: &str) -> Vec<GhStep> {
    let row = &objects.root_json["tree"][row_index(objects, name)];
    let oid = row["sha"].as_str().unwrap();
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/example/remote/git/blobs/{oid}"),
            "--hostname",
            HOST,
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--allow-escape-sequences",
        ],
        "",
    );
    step.stdout = fx::git(&fixture.repo(), &["cat-file", "blob", oid]);
    let mut operation = fx::prefix();
    operation.push(step);
    operation.extend(fx::suffix(42));
    operation
}
pub(super) fn reload_root(objects: &fx::Objects) -> Vec<GhStep> {
    let mut operation = fx::prefix();
    operation.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", objects.tree),
        objects.root_json.clone(),
    ));
    operation.extend(fx::suffix(42));
    operation
}
pub(super) fn source_row(fixture: &Fixture) -> (String, u64, bool) {
    let connection = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = connection.prepare("SELECT source,generation,active FROM sync_reservations WHERE host=?1 AND login='user-id:42' AND source LIKE 'repository-catalogue-source-v1:%' LIMIT 2").unwrap();
    let rows = statement
        .query_map([HOST], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "Exactly one original source, no invented latest source"
    );
    rows.into_iter().next().unwrap()
}
pub(super) fn saved_bytes(fixture: &Fixture) -> Vec<(String, String)> {
    let connection = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = connection.prepare("SELECT name,payload FROM saved_views WHERE host=?1 AND login='user-id:42' AND name LIKE 'repository-catalogue-intent-v1:%' ORDER BY name LIMIT 2").unwrap();
    let values = statement
        .query_map([HOST], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(values.len() <= 1 && values.iter().all(|(_, bytes)| bytes.len() <= 128 * 1024));
    values
}
pub(super) fn wait_commands(session: &mut Session, fixture: &Fixture, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        let length = commands(session, fixture).len();
        assert!(
            length <= count,
            "Unexpected/late child launched: {length}>{count}"
        );
        if length == count {
            assert_commands(session, fixture, count);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Real child journal did not reach {count}"
        );
    }
}
pub(super) fn child_reaped(session: &mut Session, pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let status = unsafe { libc::kill(pid, 0) };
        if status == -1 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
            return;
        }
        assert_eq!(
            status, 0,
            "Independent recorded process has a known lifecycle"
        );
        assert!(
            Instant::now() < deadline,
            "Cancelled original child was not reaped"
        );
        session.poll_output(Duration::from_millis(1));
    }
}
