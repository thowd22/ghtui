//! These hooks become runnable only when Tree registers the helper module.
//! Source declarations are not tool/TLS/macOS/PTY verification evidence.
use super::*;

pub fn task54_workflow_tls_v58_connect_refuses_foreign_authority_credentials_and_duplicate_host() {
    protocol::contract_hooks::task54_workflow_tls_v58_connect_refuses_foreign_authority_credentials_and_duplicate_host();
}
pub fn task54_workflow_tls_v58_only_exact_git_paths_and_bounded_protocol2_reach_backend() {
    protocol::contract_hooks::task54_workflow_tls_v58_only_exact_git_paths_and_bounded_protocol2_reach_backend();
}
pub fn task54_workflow_tls_v58_actual_cgi_body_is_unchanged_but_auth_and_ambiguous_framing_refuse()
{
    protocol::contract_hooks::task54_workflow_tls_v58_actual_cgi_body_is_unchanged_but_auth_and_ambiguous_framing_refuse();
}
pub fn task54_workflow_tls_v58_header_stream_and_count_caps_refuse_before_any_backend() {
    protocol::contract_hooks::task54_workflow_tls_v58_header_stream_and_count_caps_refuse_before_any_backend();
}

pub fn task54_workflow_tls_v58_real_tls_protocol2_lsrefs_and_create_only_push_change_only_actual_bare_ref(
) {
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let seed = root.path().join("seed");
    fs::create_dir(&seed).unwrap();
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();
    let git = executable("git").unwrap();
    let reports = Arc::new(Mutex::new(Vec::new()));
    let run = |repo: &Path, args: &[&str]| {
        process::run(
            git_command(&git, repo, args),
            &home,
            &[],
            64 * 1024,
            &reports,
        )
        .unwrap()
    };
    run(&seed, &["init", "-q", "-b", "main"]);
    fs::write(seed.join("tracked.txt"), b"actual private Git fixture\n").unwrap();
    run(&seed, &["add", "--", "tracked.txt"]);
    run(
        &seed,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "fixture original",
        ],
    );
    let original = run(&seed, &["rev-parse", "--verify", "HEAD"]);
    let server = WorkflowTlsRemoteV9::start(root.path(), &seed).unwrap();
    server
        .configure_fixture_repository(&seed, "actual-head")
        .unwrap();
    assert!(server
        .configure_fixture_repository(&seed, "actual-head")
        .is_err());
    // D is created AFTER the bare seed copy, so this proves actual receive-
    // pack/object transport, not merely adding another name for an old blob.
    fs::write(
        seed.join("tracked.txt"),
        b"actual NEW workflow fixture object\n",
    )
    .unwrap();
    run(&seed, &["add", "--", "tracked.txt"]);
    run(
        &seed,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "fixture successor",
        ],
    );
    let head = run(&seed, &["rev-parse", "--verify", "HEAD"]);
    assert_ne!(head, original);
    let head = std::str::from_utf8(&head).unwrap().trim_end_matches('\n');
    assert!(head.len() == 40 || head.len() == 64);
    let configured = run(
        &seed,
        &["remote", "get-url", "--push", "--all", "actual-head"],
    );
    assert_eq!(
        configured,
        format!("{}\n", server.canonical_url()).as_bytes()
    );
    let before = run(server.bare_repository(), &["show-ref"]);
    let listed = run(
        &seed,
        &[
            "ls-remote",
            "--refs",
            "--",
            server.canonical_url(),
            "refs/heads/helper-smoke",
        ],
    );
    assert!(listed.is_empty());
    let refspec = format!("{head}:refs/heads/helper-smoke");
    run(
        &seed,
        &[
            "push",
            "--porcelain",
            "--force-with-lease=refs/heads/helper-smoke:",
            "--",
            server.canonical_url(),
            &refspec,
        ],
    );
    assert_eq!(
        run(
            server.bare_repository(),
            &["rev-parse", "refs/heads/helper-smoke"]
        ),
        format!("{head}\n").as_bytes()
    );
    assert_eq!(
        run(server.bare_repository(), &["rev-parse", "refs/heads/main"]),
        original
    );
    assert_eq!(
        run(
            server.bare_repository(),
            &["show", &format!("{head}:tracked.txt")]
        ),
        b"actual NEW workflow fixture object\n"
    );
    let after = run(server.bare_repository(), &["show-ref"]);
    let mut expected = String::from_utf8(before)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    expected.push(format!("{head} refs/heads/helper-smoke"));
    expected.sort();
    let mut actual = String::from_utf8(after)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected);
    let requests = server.requests().unwrap();
    assert!(requests
        .iter()
        .any(|r| r.method == "GET" && r.path.ends_with("service=git-upload-pack")));
    assert!(requests
        .iter()
        .any(|r| r.method == "POST" && r.path == "/example/ci.git/git-upload-pack"));
    assert!(requests
        .iter()
        .any(|r| r.method == "POST" && r.path == "/example/ci.git/git-receive-pack"));
    assert!(requests.iter().all(|r| r.status == 200
        && r.request_bytes <= protocol::STREAM_CAP + protocol::HEADER_CAP
        && r.response_bytes <= protocol::STREAM_CAP));
    let cleanup = server.finish().unwrap();
    assert!(cleanup.exchanges <= EXCHANGES && cleanup.exchanges == cleanup.requests.len());
    assert!(!cleanup.processes.is_empty());
    for process in cleanup.processes {
        assert!(
            process.application_pid > 0
                && process.remaining.is_empty()
                && process.errors.is_empty()
        );
        assert!(owned::process_identity(process.application_pid)
            .unwrap()
            .is_none());
    }
    assert!(cleanup.io_threads_joined > 2 * cleanup.exchanges);
    assert!(reports
        .lock()
        .unwrap()
        .iter()
        .all(|r| r.errors.is_empty() && r.remaining.is_empty()));
}

pub fn task54_workflow_tls_v58_unsafe_root_and_seed_escape_refuse_before_child_or_config() {
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let foreign = tempfile::tempdir().unwrap();
    assert!(WorkflowTlsRemoteV9::start(root.path(), foreign.path()).is_err());
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(root.path(), &alias).unwrap();
    assert!(WorkflowTlsRemoteV9::start(&alias, root.path()).is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);

    // Exercise the actual pre-Self RAII guard at an ordinary setup-error
    // boundary. These descriptive faults never create process authority or
    // signal/reap any PID; only files under this disposable root are observed.
    for mode in 0..4 {
        let directory = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(root.path())
            .unwrap();
        let path = directory.path().to_owned();
        fs::write(path.join("evidence"), b"unchanged private setup evidence").unwrap();
        let reports = Arc::new(Mutex::new(Vec::new()));
        if mode == 1 || mode == 2 {
            let mut report = owned::CleanupReport {
                application_pid: unsafe { libc::getpid() },
                ..Default::default()
            };
            if mode == 1 {
                report.errors.push("Session absence unproven".into());
            } else {
                report.remaining.push(report.application_pid);
            }
            reports.lock().unwrap().push(report);
        } else if mode == 3 {
            let poison = reports.clone();
            assert!(std::panic::catch_unwind(move || {
                let _lock = poison.lock().unwrap();
                panic!("deliberate cleanup-report poisoning");
            })
            .is_err());
        }
        let setup_result: Result<(), &str> = {
            let _guard = StartDirectory {
                directory: Some(directory),
                reports,
            };
            Err("ordinary setup failure")
        };
        assert_eq!(setup_result, Err("ordinary setup failure"));
        if mode == 0 {
            assert!(!path.exists(), "proven/no-child setup failure must clean");
        } else {
            assert_eq!(
                fs::read(path.join("evidence")).unwrap(),
                b"unchanged private setup evidence"
            );
        }
    }
}

pub fn task54_workflow_tls_v58_cancel_actual_connect_reaps_owned_openssl_and_joins_all_io_without_rpc(
) {
    let root = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let seed = root.path().join("seed");
    fs::create_dir(&seed).unwrap();
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();
    let git = executable("git").unwrap();
    let reports = Arc::new(Mutex::new(Vec::new()));
    process::run(
        git_command(&git, &seed, &["init", "-q", "-b", "main"]),
        &home,
        &[],
        64 * 1024,
        &reports,
    )
    .unwrap();
    let server = WorkflowTlsRemoteV9::start(root.path(), &seed).unwrap();
    let mut client = TcpStream::connect_timeout(&server.proxy, Duration::from_secs(1)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    client
        .write_all(b"CONNECT ghe.example:443 HTTP/1.1\r\nHost: ghe.example:443\r\n\r\n")
        .unwrap();
    let mut response = [0u8; 39];
    client.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"HTTP/1.1 200 Connection Established\r\n\r\n");
    // This actual200 follows positive OpenSSL session capture/listen. No
    // encrypted application bytes or RPC are sent: cancel must reap it.
    let cleanup = server.finish().unwrap();
    assert_eq!(cleanup.exchanges, 1);
    assert!(cleanup.requests.is_empty());
    assert!(cleanup.io_threads_joined >= 3);
    assert!(cleanup.processes.len() >= 5);
    for report in cleanup.processes {
        assert!(
            report.application_pid > 0 && report.errors.is_empty() && report.remaining.is_empty()
        );
        assert!(owned::process_identity(report.application_pid)
            .unwrap()
            .is_none());
    }
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).unwrap(), 0);
}
