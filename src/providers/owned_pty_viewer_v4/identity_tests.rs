use super::*;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::owned_pty_processes_test_support as observer;

struct Held {
    child: Box<dyn Child + Send + Sync>,
    owner: OwnedSession,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}
impl Held {
    fn spawn(script: &str) -> Self {
        let pair = native_pty_system().openpty(PtySize::default()).unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", script]);
        let child = pair.slave.spawn_command(command).unwrap();
        let owner = OwnedSession::capture(child.as_ref()).unwrap();
        Self {
            child,
            owner,
            _master: pair.master,
        }
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.owner.cleanup(self.child.as_mut());
    }
}

#[test]
fn native_external_diff_owned_pty_is_distinct_real_session_and_leader_really_reaped() {
    let mut held = Held::spawn("printf ready; read answer");
    let original = identity(held.owner.leader.pid).unwrap().unwrap();
    assert_eq!(original.sid, original.pid);
    assert_eq!(original.parent, unsafe { libc::getpid() });
    assert_ne!(original.sid, unsafe { libc::getsid(0) });
    let report = held.owner.cleanup(held.child.as_mut());
    assert!(report.proven(), "{report:?}");
    assert!(report.reaped.contains(&original.pid));
    assert_eq!(identity(original.pid).unwrap(), None);
}
#[test]
fn native_external_diff_owned_cleanup_includes_other_groups_but_never_a_foreign_session() {
    // Test-only orphan adoption, never the production helper/global policy.
    observer::initialize_reaping().unwrap();
    let mut held = Held::spawn("set -m; sleep 60 & wait");
    let mut foreign = Held::spawn("read answer");
    let foreign_before = identity(foreign.owner.leader.pid).unwrap().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let members = loop {
        let members = held.owner.members(deadline).unwrap();
        if members.len() >= 2 {
            break members;
        }
        assert!(
            Instant::now() < deadline,
            "real child never entered owned session"
        );
        std::thread::yield_now();
    };
    let child = members
        .iter()
        .find(|p| p.pid != held.owner.leader.pid)
        .unwrap();
    assert_ne!(
        unsafe { libc::getpgid(child.pid) },
        unsafe { libc::getpgid(held.owner.leader.pid) },
        "fixture must genuinely create another process group"
    );
    let report = held.owner.cleanup(held.child.as_mut());
    assert!(report.proven(), "{report:?}");
    for member in members {
        assert_eq!(identity(member.pid).unwrap(), None);
    }
    assert_eq!(identity(foreign_before.pid).unwrap(), Some(foreign_before));
    assert!(foreign.owner.cleanup(foreign.child.as_mut()).proven());
}
#[test]
fn native_external_diff_owned_identity_refuses_unrelated_real_process_without_signal() {
    let held = Held::spawn("read answer");
    let parent = identity(unsafe { libc::getpid() }).unwrap().unwrap();
    assert!(!held.owner.owns(&parent));
    assert!(held.owner.signal(&parent, libc::SIGSTOP).is_err());
    assert_eq!(identity(parent.pid).unwrap(), Some(parent));
}

struct InventoryObserverGuard;
impl Drop for InventoryObserverGuard {
    fn drop(&mut self) {
        INVENTORY_OBSERVER.with(|observer| *observer.borrow_mut() = None);
    }
}
fn entered(path: &std::path::Path, generation: usize) -> io::Result<i32> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match std::fs::read(path.join(format!("entered-{generation}"))) {
            Ok(bytes) => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|_| invalid("actual descendant entry unavailable"))?;
                let pid = value["pid"]
                    .as_i64()
                    .and_then(|pid| i32::try_from(pid).ok())
                    .filter(|pid| *pid > 0)
                    .ok_or_else(|| invalid("actual positive descendant PID unavailable"))?;
                return Ok(pid);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if Instant::now() >= deadline {
            return Err(invalid("actual descendant never entered original session"));
        }
        std::thread::yield_now();
    }
}
fn controlled_growth(growth: usize, expect_proven: bool) {
    observer::initialize_reaping().unwrap();
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = std::str::from_utf8(&node.stdout).unwrap().trim();
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("actual-descendants.js");
    let source=format!("'use strict';const fs=require('node:fs'),cp=require('node:child_process');const root={},generation=Number(process.argv[2]);fs.writeFileSync(root+'/entering-'+generation,JSON.stringify({{pid:process.pid,parent:process.ppid}}));fs.renameSync(root+'/entering-'+generation,root+'/entered-'+generation);function child(){{cp.spawn(process.execPath,[process.argv[1],String(generation+1)],{{stdio:'ignore'}})}}if(generation===0)child();let released=false;setInterval(()=>{{if(generation>0&&generation<{}&&!released&&fs.existsSync(root+'/permit-'+generation)){{released=true;child()}}}},2);",serde_json::to_string(directory.path()).unwrap(),growth+1);
    std::fs::write(&program, source).unwrap();
    let pair = native_pty_system().openpty(PtySize::default()).unwrap();
    let mut command = CommandBuilder::new(node);
    command.args([program.as_os_str(), std::ffi::OsStr::new("0")]);
    let child = pair.slave.spawn_command(command).unwrap();
    let owner = OwnedSession::capture(child.as_ref()).unwrap();
    drop(pair.slave);
    let mut held = Held {
        child,
        owner,
        _master: pair.master,
    };
    let first = entered(directory.path(), 1).unwrap();
    assert_eq!(unsafe { libc::getsid(first) }, held.owner.leader.sid);
    let mut foreign = Held::spawn("read answer");
    let foreign_before = identity(foreign.owner.leader.pid).unwrap().unwrap();
    let root = directory.path().to_owned();
    let sid = held.owner.leader.sid;
    let mut pass = 0;
    INVENTORY_OBSERVER.with(|observer| {
        *observer.borrow_mut() = Some(Box::new(move || {
            if pass < growth {
                let generation = pass + 1;
                std::fs::write(
                    root.join(format!("permit-{generation}")),
                    b"fork ONLY this actual next generation",
                )?;
                let pid = entered(&root, generation + 1)?;
                if unsafe { libc::getsid(pid) } != sid {
                    return Err(invalid("real descendant escaped the original SID"));
                }
            }
            pass += 1;
            Ok(())
        }))
    });
    let observer_guard = InventoryObserverGuard;
    let report = held.owner.cleanup(held.child.as_mut());
    drop(observer_guard);
    assert_eq!(report.proven(), expect_proven, "{report:?}");
    if !expect_proven {
        assert!(
            report
                .errors
                .iter()
                .any(|error| error.contains("fixed point NOT proven")),
            "final growth may not be silently Clean: {report:?}"
        );
    }
    assert!(
        report.remaining.is_empty(),
        "all positively captured original identities still get actual cleanup: {report:?}"
    );
    for generation in 0..=growth + 1 {
        let pid = entered(directory.path(), generation).unwrap();
        assert_eq!(
            identity(pid).unwrap(),
            None,
            "actual original generation{generation} must be absent, not ECHILD or kill0"
        );
    }
    assert_eq!(identity(foreign_before.pid).unwrap(), Some(foreign_before));
    assert!(foreign.owner.cleanup(foreign.child.as_mut()).proven());
}
#[test]
fn native_external_diff_owned_real_late_descendant_fixed_point_closes_before_leader_reap() {
    controlled_growth(1, true);
}
#[test]
fn native_external_diff_owned_real_fourth_pass_growth_cleans_exact_children_but_remains_unproven() {
    controlled_growth(4, false);
}
#[cfg(target_os = "linux")]
#[test]
fn native_external_diff_owned_linux_stat_keeps_real_birth_and_refuses_malformed_identity() {
    let pid = unsafe { libc::getpid() };
    let bytes = std::fs::read(format!("/proc/{pid}/stat")).unwrap();
    let parsed = parse_stat(&bytes, unsafe { libc::geteuid() }).unwrap();
    assert_eq!(parsed.pid, pid);
    assert_eq!(parsed.sid, unsafe { libc::getsid(0) });
    assert!(parsed.birth.0 > 0);
    assert!(parse_stat(b"1 (not closed", 0).is_err());
    assert!(parse_stat(&vec![b'0'; MAX_STAT + 1], 0).is_err());
    assert!(finish(Err(io::Error::from_raw_os_error(libc::EACCES))).is_err());
    assert_eq!(
        finish(Err(io::Error::from_raw_os_error(libc::ESRCH))).unwrap(),
        None
    );
}
