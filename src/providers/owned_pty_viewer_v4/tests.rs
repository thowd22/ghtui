use super::*;
use std::process::Command;

pub(crate) struct Fixture {
    _directory: tempfile::TempDir,
    pub(crate) program: PathBuf,
}
impl Fixture {
    pub(crate) fn pager(&self) -> VerifiedPagerV4 {
        let result = Command::new(&self.program)
            .arg("--version")
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout, b"less 710\n");
        VerifiedPagerV4::from_version(self.program.clone(), &result.stdout).unwrap()
    }
}
pub(crate) fn fixture(extra: &str) -> Fixture {
    // Strict offline executable fixture; actual Node, real child PTY, no mock
    // Child/Screen/ownership. Version is obtained from the actual fixture.
    let node = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = std::str::from_utf8(&node.stdout).unwrap().trim();
    assert!(Path::new(node).is_absolute());
    assert!(!node.contains('\n'));
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("less-fixture");
    let source=format!("#!{node}\n'use strict';\nconst fs=require('node:fs');\nif(process.argv.length===3&&process.argv[2]==='--version'){{process.stdout.write('less 710\\n');process.exit(0)}}\nconst args=process.argv.slice(2);\nif(args.length!==5||JSON.stringify(args.slice(0,4))!==JSON.stringify(['-R','-K','-L','--'])) process.exit(81);\nif(process.env.LESSSECURE!=='1'||process.env.LESSSECURE_ALLOW!==''||process.env.LESSHISTFILE!=='/dev/null'||process.env.SHELL!=='/bin/false')process.exit(82);\nfor(const key of ['GH_TOKEN','GITHUB_TOKEN','LESSOPEN','LESSCLOSE','LESSKEY','PAGER','LD_PRELOAD','DYLD_INSERT_LIBRARIES'])if(process.env[key]!==undefined)process.exit(83);\nif(!process.stdin.isTTY||!process.stdout.isTTY||!process.stderr.isTTY)process.exit(84);\nconst input=fs.readFileSync(args[4]);\nfs.writeFileSync(args[4]+'.entered.json',JSON.stringify({{pid:process.pid,parent:process.ppid,stdin:process.stdin.isTTY,stdout:process.stdout.isTTY,stderr:process.stderr.isTTY,input:input.toString('hex')}}));\nprocess.stdin.setRawMode(true);process.stdin.resume();\nfunction draw(text){{process.stdout.write('\\x1b[2J\\x1b[H'+text)}}\nprocess.stdout.on('resize',()=>draw('SIZE '+process.stdout.columns+'x'+process.stdout.rows));\n{extra}\ndraw('READY '+input.toString('utf8'));\nprocess.stdin.on('data',bytes=>draw('KEY '+JSON.stringify(bytes.toString('utf8'))));\n");
    std::fs::write(&program, source).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
    Fixture {
        _directory: directory,
        program,
    }
}
pub(crate) fn wait_frame(handle: &PtyViewerHandleV4, needle: &str) -> Arc<ViewerSnapshotV4> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(frame) = handle.snapshot() {
            let text: String = frame.cells.iter().map(|cell| cell.text.as_str()).collect();
            if text.contains(needle) {
                return frame;
            }
        }
        assert!(
            Instant::now() < deadline,
            "actual child frame missing {needle}"
        );
        thread::sleep(Duration::from_millis(5));
    }
}
pub(crate) fn wait_exit(handle: &mut PtyViewerHandleV4) -> ViewerExitV4 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(exit) = handle.exit() {
            return exit;
        }
        assert!(
            Instant::now() < deadline,
            "actual child cleanup never completed"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn native_external_diff_private_capture_actual_bytes_modes_inodes_and_drop_cleanup() {
    let capture =
        PreparedPagerCaptureV4::from_renderer(b"a\x1b[31m-b\x1b[0m\x1b]52;c;secret\x07\r").unwrap();
    assert_eq!(
        std::fs::read(&capture.path).unwrap(),
        b"a\x1b[31m-b\x1b[0m\\r"
    );
    assert_eq!(
        std::fs::metadata(&capture.path).unwrap().mode() & 0o777,
        0o400
    );
    assert_eq!(
        std::fs::metadata(capture.directory.path()).unwrap().mode() & 0o777,
        0o700
    );
    assert_eq!(std::fs::metadata(&capture.path).unwrap().nlink(), 1);
    assert_eq!(
        capture.bytes(),
        std::fs::metadata(&capture.path).unwrap().len() as usize
    );
    capture.validate().unwrap();
    let directory = capture.directory.path().to_owned();
    capture.close().unwrap();
    assert!(!directory.exists());
}
#[test]
fn native_external_diff_capture_replaced_file_and_directory_refuse_before_launch() {
    let capture = PreparedPagerCaptureV4::from_renderer(b"private-original").unwrap();
    let original = capture.directory.path().join("original");
    std::fs::rename(&capture.path, &original).unwrap();
    std::fs::write(&capture.path, b"private-original").unwrap();
    std::fs::set_permissions(&capture.path, std::fs::Permissions::from_mode(0o400)).unwrap();
    assert!(capture.validate().is_err());
    let capture = PreparedPagerCaptureV4::from_renderer(b"private-original").unwrap();
    let moved = capture.directory.path().with_extension("moved");
    std::fs::rename(capture.directory.path(), &moved).unwrap();
    std::fs::create_dir(capture.directory.path()).unwrap();
    std::fs::set_permissions(
        capture.directory.path(),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::rename(moved.join("rendered.diff"), &capture.path).unwrap();
    assert!(capture.validate().is_err());
    std::fs::remove_dir(&moved).unwrap();
}
#[test]
fn native_external_diff_cancelled_original_display_permit_starts_zero_pagers() {
    let fixture = fixture("");
    let capture = PreparedPagerCaptureV4::from_renderer(b"unchanged").unwrap();
    let path = capture.directory.path().to_owned();
    let permit = DisplayPermitV4::launch();
    permit.cancellation().cancel();
    let admission = ViewerAdmissionV4::new();
    assert!(PtyViewerHandleV4::launch(
        capture,
        fixture.pager(),
        permit,
        ViewerGeometryV4::new(80, 12, 0).unwrap(),
        &admission
    )
    .is_err());
    assert!(!admission.blocked());
    assert!(!path.exists());
}
#[test]
fn native_external_diff_fixed_commands_geometry_and_mechanical_admission_never_grant_authority() {
    assert_eq!(ViewerCommandV4::Line(1).bytes().unwrap(), b"j");
    assert_eq!(
        ViewerCommandV4::Search("needle界".into()).bytes().unwrap(),
        "/needle界\n".as_bytes()
    );
    for command in [
        ViewerCommandV4::Line(0),
        ViewerCommandV4::Search("!shell\ncommand".into()),
        ViewerCommandV4::Search("x".repeat(255)),
    ] {
        assert!(command.bytes().is_err());
    }
    for (cols, rows) in [(0, 4), (241, 4), (80, 0), (80, 61)] {
        assert!(ViewerGeometryV4::new(cols, rows, 0).is_err());
    }
    let admission = ViewerAdmissionV4::new();
    let mut ticket = admission.acquire().unwrap();
    assert!(admission.acquire().is_err());
    ticket.finish(false);
    assert!(admission.acquire().is_err());
    assert!(admission.blocked());
}
#[test]
fn native_external_diff_real_pager_pty_key_resize_cancel_and_temporary_cleanup() {
    let fixture = fixture("");
    let capture = PreparedPagerCaptureV4::from_renderer(b"original-genuine-capture").unwrap();
    let directory = capture.directory.path().to_owned();
    let evidence = capture.path.with_extension("diff.entered.json");
    let admission = ViewerAdmissionV4::new();
    let first = ViewerGeometryV4::new(80, 12, 1).unwrap();
    let mut handle = PtyViewerHandleV4::launch(
        capture,
        fixture.pager(),
        DisplayPermitV4::launch(),
        first,
        &admission,
    )
    .unwrap();
    let actual = wait_frame(&handle, "READY original-genuine-capture");
    assert!(actual.belongs(handle.owner(), first));
    let entered: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&evidence).unwrap()).unwrap();
    assert_eq!(entered["stdin"], true);
    assert_eq!(entered["stdout"], true);
    assert_eq!(entered["stderr"], true);
    let pid = entered["pid"].as_i64().unwrap() as i32;
    assert_eq!(unsafe { libc::getsid(pid) }, pid);
    assert_ne!(unsafe { libc::getsid(0) }, pid);
    assert_eq!(
        entered["input"],
        "6f726967696e616c2d67656e75696e652d63617074757265"
    );
    handle.command(ViewerCommandV4::Line(1)).unwrap();
    wait_frame(&handle, "KEY \"j\"");
    let next = ViewerGeometryV4::new(40, 8, 2).unwrap();
    handle.resize(next);
    let resized = wait_frame(&handle, "SIZE 40x8");
    assert!(resized.belongs(handle.owner(), next));
    assert!(!actual.belongs(handle.owner(), next));
    assert!(resized.retained < actual.retained);
    assert_eq!(
        handle.shared.snapshot_peak.load(Ordering::Relaxed),
        actual.retained,
        "shrinking never forgets an actual older snapshot allocation"
    );
    handle.cancel();
    let exit = wait_exit(&mut handle);
    assert!(exit.cleanup_proven, "{}", exit.reason);
    assert_eq!(exit.leader, pid);
    assert_eq!(unsafe { libc::getsid(pid) }, -1);
    assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    assert!(!directory.exists());
    assert!(!admission.blocked());
    assert!(handle.snapshot().is_none());
}
#[test]
fn native_external_diff_real_pager_exit_two_is_not_success_and_drops_private_input() {
    let fixture = fixture("process.exit(2);");
    let capture = PreparedPagerCaptureV4::from_renderer(b"private").unwrap();
    let directory = capture.directory.path().to_owned();
    let admission = ViewerAdmissionV4::new();
    let mut handle = PtyViewerHandleV4::launch(
        capture,
        fixture.pager(),
        DisplayPermitV4::launch(),
        ViewerGeometryV4::new(80, 12, 0).unwrap(),
        &admission,
    )
    .unwrap();
    let exit = wait_exit(&mut handle);
    assert!(exit.reason.contains("exited2"), "{}", exit.reason);
    assert!(exit.cleanup_proven);
    assert!(!directory.exists());
    assert!(!admission.blocked());
}
#[test]
fn native_external_diff_real_pager_output_is_emulated_cells_not_host_title_or_clipboard() {
    let fixture = fixture(
        "process.stdout.write('\\x1b]0;PRIVATE-TITLE\\x07\\x1b]52;c;CLIPBOARD\\x07\\x1b[?1003h');",
    );
    let capture = PreparedPagerCaptureV4::from_renderer(b"safe-body").unwrap();
    let admission = ViewerAdmissionV4::new();
    let mut handle = PtyViewerHandleV4::launch(
        capture,
        fixture.pager(),
        DisplayPermitV4::launch(),
        ViewerGeometryV4::new(80, 12, 0).unwrap(),
        &admission,
    )
    .unwrap();
    let frame = wait_frame(&handle, "READY safe-body");
    let text: String = frame.cells.iter().map(|cell| cell.text.as_str()).collect();
    assert!(!text.contains("PRIVATE-TITLE"));
    assert!(!text.contains("CLIPBOARD"));
    assert!(!text.contains('\x1b'));
    handle.cancel();
    assert!(wait_exit(&mut handle).cleanup_proven);
}
#[test]
fn native_external_diff_real_pager_original_expiry_reaps_actual_entered_child() {
    let fixture = fixture("");
    let capture = PreparedPagerCaptureV4::from_renderer(b"deadline").unwrap();
    let admission = ViewerAdmissionV4::new();
    // Private test clock boundary, not an application/environment override.
    let permit = Arc::new(DisplayPermitV4 {
        deadline: Instant::now() + Duration::from_secs(3),
        cancel: Default::default(),
    });
    let mut handle = PtyViewerHandleV4::launch(
        capture,
        fixture.pager(),
        permit,
        ViewerGeometryV4::new(80, 12, 0).unwrap(),
        &admission,
    )
    .unwrap();
    wait_frame(&handle, "READY deadline");
    let exit = wait_exit(&mut handle);
    assert!(exit.reason.contains("permit expired"), "{}", exit.reason);
    assert!(exit.leader > 0);
    assert!(exit.cleanup_proven);
    assert!(!admission.blocked());
}
