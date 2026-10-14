//! Offline held provider fixture: two separate PGIDs, one genuine PTY SID.
use std::{fs, os::unix::process::CommandExt, process::{Command, Stdio}, thread, time::{Duration,Instant}};
unsafe extern "C" { fn getsid(pid:i32)->i32; fn getpgid(pid:i32)->i32; fn getppid()->i32; }
fn record(name:&str) {
    let root=std::env::var_os("GHTUI_OWNED_PROCESS_FIXTURE").unwrap();
    fs::write(std::path::Path::new(&root).join(name),format!("{} {} {} {}",std::process::id(),unsafe{getppid()},unsafe{getpgid(0)},unsafe{getsid(0)})).unwrap();
}
fn main() {
    let worker=std::env::args().nth(1).as_deref()==Some("--owned-held-worker");
    let mut child=if worker {None} else {
        assert_eq!(std::env::args().skip(1).collect::<Vec<_>>(),vec!["--version"]);
        Some(Command::new(std::env::current_exe().unwrap()).arg("--owned-held-worker")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn().unwrap())
    };
    record(if worker {"held-grandchild.pid"} else {"held-parent.pid"});
    let deadline=Instant::now()+Duration::from_secs(60);
    while Instant::now()<deadline {thread::sleep(Duration::from_millis(10));}
    if let Some(child)=child.as_mut(){let _=child.kill();let _=child.wait();}
}
