//! PRIVATE ownership of the one directly spawned external-viewer PTY session.
//! No process-wide subreaper, name matching, group signal, or arbitrary PID API.
use portable_pty::Child;
use std::{
    collections::BTreeMap,
    io,
    time::{Duration, Instant},
};

const MAX_SCANNED: usize = 65_536;
const MAX_OWNED: usize = 64;
#[cfg(target_os = "linux")]
const MAX_STAT: usize = 16_384;
const CLEANUP_TIME: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, Eq)]
struct Process {
    pid: i32,
    parent: i32,
    sid: i32,
    uid: u32,
    birth: (u64, u64),
    zombie: bool,
}
impl Process {
    fn same(&self, other: &Self) -> bool {
        (self.pid, self.sid, self.uid, self.birth) == (other.pid, other.sid, other.uid, other.birth)
    }
}

#[derive(Debug, Default)]
pub(super) struct Cleanup {
    pub(super) leader: i32,
    pub(super) exit_code: Option<u32>,
    pub(super) reaped: Vec<i32>,
    pub(super) gone: Vec<i32>,
    pub(super) remaining: Vec<i32>,
    pub(super) errors: Vec<String>,
}
impl Cleanup {
    pub(super) fn proven(&self) -> bool {
        self.remaining.is_empty() && self.errors.is_empty()
    }
    fn error(&mut self, error: impl std::fmt::Display) {
        // Fixed report bound; do not retain arbitrary child data or paths.
        if self.errors.len() < MAX_OWNED * 4 + 8 {
            self.errors
                .push(error.to_string().chars().take(192).collect());
        }
    }
}

#[derive(Debug)]
pub(super) struct OwnedSession {
    leader: Process,
}
impl OwnedSession {
    pub(super) fn capture(child: &dyn Child) -> io::Result<Self> {
        let pid = child
            .process_id()
            .and_then(|pid| i32::try_from(pid).ok())
            .filter(|pid| *pid > 0)
            .ok_or_else(|| invalid("viewer has no positive direct-child PID"))?;
        let leader = identity(pid)?.ok_or_else(|| invalid("viewer leader disappeared"))?;
        // SAFETY: these functions only inspect the calling process.
        let current = unsafe { libc::getpid() };
        if pid == current
            || leader.parent != current
            || leader.sid != pid
            || leader.uid != unsafe { libc::geteuid() }
            || unsafe { libc::getsid(0) } == leader.sid
        {
            return Err(invalid(
                "viewer is not our distinct direct-child PTY session",
            ));
        }
        Ok(Self { leader })
    }

    fn owns(&self, process: &Process) -> bool {
        process.pid > 0
            && process.pid != unsafe { libc::getpid() }
            && process.sid == self.leader.sid
            && process.uid == self.leader.uid
            && process.birth >= self.leader.birth
            && (process.pid != self.leader.pid || self.leader.same(process))
    }

    fn members(&self, deadline: Instant) -> io::Result<Vec<Process>> {
        if identity(self.leader.pid)?.is_some_and(|now| !self.leader.same(&now)) {
            return Err(invalid("original viewer leader identity was replaced"));
        }
        let mut members = Vec::new();
        let candidates = pids()?;
        // Test-only observer may coordinate REAL descendants after this exact
        // kernel inventory snapshot, but supplies no PID/identity/absence.
        #[cfg(test)]
        INVENTORY_OBSERVER.with(|observer| {
            if let Some(observer) = observer.borrow_mut().as_mut() {
                observer()
            } else {
                Ok(())
            }
        })?;
        for pid in candidates {
            if Instant::now() >= deadline {
                return Err(invalid("viewer process inventory deadline expired"));
            }
            // A live member reserves its SID, including a different process
            // group. A foreign session is never inferred from parent/name.
            if unsafe { libc::getsid(pid) } != self.leader.sid {
                continue;
            }
            if let Some(process) = identity(pid)? {
                if self.owns(&process) {
                    if members.len() == MAX_OWNED {
                        return Err(invalid("viewer session exceeds owned-process bound"));
                    }
                    members.push(process);
                }
            }
        }
        Ok(members)
    }

    fn signal(&self, expected: &Process, signal: i32) -> io::Result<bool> {
        if !self.owns(expected) {
            return Err(invalid("refusing a foreign viewer process"));
        }
        let Some(now) = identity(expected.pid)? else {
            return Ok(false);
        };
        if !expected.same(&now) {
            return Err(invalid("viewer process identity changed before signal"));
        }
        if now.zombie {
            return Ok(false);
        }
        // SAFETY: positive individually revalidated owned PID, never a group.
        if unsafe { libc::kill(expected.pid, signal) } == 0 {
            Ok(true)
        } else {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                Ok(false)
            } else {
                Err(error)
            }
        }
    }

    /// Always attempts cleanup of EVERY verified child, even after inventory,
    /// signal, deadline or wait errors. No frozen child is silently abandoned.
    pub(super) fn cleanup(&self, child: &mut dyn Child) -> Cleanup {
        let mut report = Cleanup {
            leader: self.leader.pid,
            ..Default::default()
        };
        let deadline = Instant::now() + CLEANUP_TIME;
        let mut known = BTreeMap::from([(self.leader.pid, self.leader.clone())]);
        let mut fixed_point = false;
        if let Err(error) = self.signal(&self.leader, libc::SIGSTOP) {
            report.error(error);
        }
        for _ in 0..4 {
            match self.members(deadline) {
                Ok(members) => {
                    let mut changed = false;
                    for process in members {
                        if !known.contains_key(&process.pid) && known.len() == MAX_OWNED {
                            report.error("viewer cleanup inventory reached its bound");
                            break;
                        }
                        if let Some(previous) = known.get(&process.pid) {
                            if !previous.same(&process) {
                                changed = true;
                                report.error("viewer PID identity changed during inventory; closure unproven");
                            }
                        } else {
                            changed = true;
                        }
                        // Record exact identity BEFORE attempting SIGSTOP, so
                        // every later error still reaches its kill/resume pass.
                        known.insert(process.pid, process.clone());
                        if let Err(error) = self.signal(&process, libc::SIGSTOP) {
                            report.error(error);
                        }
                    }
                    if !changed {
                        fixed_point = true;
                        break;
                    }
                }
                Err(error) => {
                    report.error(error);
                    break;
                }
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        if !fixed_point {
            report.error("viewer cleanup inventory fixed point NOT proven within4 passes/deadline");
        }
        // Another snapshot after all known members were individually stopped.
        // Late growth is cleaned, but can NEVER erase exhausted/uncertain proof.
        match self.members(deadline) {
            Ok(members) => {
                for process in members {
                    if known.contains_key(&process.pid) {
                        continue;
                    }
                    report.error("late original viewer member invalidates cleanup fixed point");
                    if known.len() == MAX_OWNED {
                        report.error("late viewer inventory exceeds bound");
                        continue;
                    }
                    known.insert(process.pid, process.clone());
                    if let Err(error) = self.signal(&process, libc::SIGSTOP) {
                        report.error(error);
                    }
                }
            }
            Err(error) => report.error(error),
        }
        // Never return between freezing a verified child and this complete
        // cleanup pass. Even an expired inventory clock does not skip a PID.
        for process in known.values() {
            if let Err(error) = self.signal(process, libc::SIGKILL) {
                report.error(error);
                // If a kill failed, do not silently leave our verified live
                // child stopped. Revalidate again; never resume a replacement.
                if let Err(error) = self.signal(process, libc::SIGCONT) {
                    report.error(error);
                }
            }
        }
        let mut final_observed = false;
        let mut final_passes = 0;
        while !known.is_empty() && Instant::now() < deadline {
            // Keep the actual direct leader UNREAPED as an original PID/SID
            // reservation until all other captured identities are absent and
            // a final bounded same-session observation has closed. This is
            // conservative bounded evidence, not a race-free OS containment
            // or an assertion that an arbitrary forking program is sandboxed.
            if known.len() == 1 && known.contains_key(&self.leader.pid) && !final_observed {
                final_passes += 1;
                if final_passes > 4 {
                    report.error("final viewer closure inventory bound exhausted");
                    break;
                }
                match identity(self.leader.pid) {
                    Ok(Some(current)) if self.leader.same(&current) => {}
                    Ok(_) => {
                        report.error(
                            "original viewer leader reservation absent/changed before closure",
                        );
                        break;
                    }
                    Err(error) => {
                        report.error(error);
                        break;
                    }
                }
                match self.members(deadline) {
                    Ok(members) => {
                        let mut late = false;
                        for process in members {
                            if known.contains_key(&process.pid) {
                                continue;
                            }
                            late = true;
                            report.error("late original viewer member prevents proven closure");
                            if known.len() == MAX_OWNED {
                                report.error("final viewer inventory exceeds bound");
                                continue;
                            }
                            known.insert(process.pid, process.clone());
                            if let Err(error) = self.signal(&process, libc::SIGSTOP) {
                                report.error(error);
                            }
                            if let Err(error) = self.signal(&process, libc::SIGKILL) {
                                report.error(error);
                                if let Err(error) = self.signal(&process, libc::SIGCONT) {
                                    report.error(error);
                                }
                            }
                        }
                        if late {
                            continue;
                        }
                        final_observed = true;
                    }
                    Err(error) => {
                        report.error(error);
                        break;
                    }
                }
            }
            let mut completed = Vec::new();
            for (&pid, expected) in &known {
                if pid == self.leader.pid {
                    if !final_observed {
                        continue;
                    }
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            report.exit_code = Some(status.exit_code());
                            report.reaped.push(pid);
                            completed.push(pid);
                        }
                        Ok(None) => {}
                        Err(error) => report.error(error),
                    }
                    continue;
                }
                match identity(pid) {
                    Ok(None) => {
                        report.gone.push(pid);
                        completed.push(pid);
                    }
                    Ok(Some(now)) if !expected.same(&now) => {
                        // The original instance is gone. Do not signal/reap
                        // the independently reused PID or claim it is absent.
                        report.error("viewer PID reused before absence could be proven");
                        completed.push(pid);
                    }
                    Ok(Some(now)) => {
                        if now.zombie && now.parent == unsafe { libc::getpid() } {
                            // SAFETY: only this exact adopted owned zombie;
                            // no global subreaper or waitpid(-1) behavior.
                            let waited =
                                unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
                            if waited == pid {
                                report.reaped.push(pid);
                                completed.push(pid);
                            } else if waited < 0 {
                                let error = io::Error::last_os_error();
                                if error.raw_os_error() != Some(libc::ECHILD) {
                                    report.error(error);
                                }
                                // ECHILD is NOT absence or successful reaping.
                            }
                        }
                    }
                    Err(error) => report.error(error),
                }
            }
            for pid in completed {
                known.remove(&pid);
            }
            if !known.is_empty() {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        if !final_observed {
            report.error("final original viewer session closure NOT proven");
        }
        // Even an unproven/exhausted inventory must attempt its positively
        // owned direct-child wait. ECHILD is never interpreted as absence.
        if known.contains_key(&self.leader.pid) {
            match child.try_wait() {
                Ok(Some(status)) => {
                    report.exit_code = Some(status.exit_code());
                    report.reaped.push(self.leader.pid);
                    known.remove(&self.leader.pid);
                }
                Ok(None) => {}
                Err(error) => report.error(error),
            }
        }
        report.remaining = known.into_keys().collect();
        report
    }
}

#[cfg(test)]
type InventoryObserverV4 = Box<dyn FnMut() -> io::Result<()>>;
#[cfg(test)]
std::thread_local! {static INVENTORY_OBSERVER:std::cell::RefCell<Option<InventoryObserverV4>>=std::cell::RefCell::new(None);}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn finish(result: io::Result<Process>) -> io::Result<Option<Process>> {
    match result {
        Ok(process) => Ok(Some(process)),
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ESRCH) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

#[cfg(target_os = "linux")]
fn parse_stat(bytes: &[u8], uid: u32) -> io::Result<Process> {
    if bytes.len() > MAX_STAT {
        return Err(invalid("viewer stat exceeds bound"));
    }
    let open = bytes
        .iter()
        .position(|b| *b == b'(')
        .ok_or_else(|| invalid("missing stat command"))?;
    let close = bytes
        .windows(2)
        .rposition(|b| b == b") ")
        .filter(|end| *end > open)
        .ok_or_else(|| invalid("missing stat command end"))?;
    let number = |text: &str| text.parse::<i32>().map_err(|_| invalid("invalid stat PID"));
    let pid = number(
        std::str::from_utf8(&bytes[..open])
            .map_err(|_| invalid("invalid stat PID bytes"))?
            .trim(),
    )?;
    let mut tail = std::str::from_utf8(&bytes[close + 2..])
        .map_err(|_| invalid("invalid stat fields"))?
        .split_ascii_whitespace();
    let state = tail.next().ok_or_else(|| invalid("missing stat state"))?;
    if state.len() != 1 || !b"RSDZTtXxKWPI".contains(&state.as_bytes()[0]) || pid <= 0 {
        return Err(invalid("invalid stat state/PID"));
    }
    let parent = number(tail.next().ok_or_else(|| invalid("missing parent"))?)?;
    tail.next().ok_or_else(|| invalid("missing group"))?;
    let sid = number(tail.next().ok_or_else(|| invalid("missing session"))?)?;
    let birth = tail
        .nth(15)
        .ok_or_else(|| invalid("missing process birth"))?
        .parse::<u64>()
        .map_err(|_| invalid("invalid process birth"))?;
    if parent < 0 || sid <= 0 || birth == 0 {
        return Err(invalid("invalid process ownership"));
    }
    Ok(Process {
        pid,
        parent,
        sid,
        uid,
        birth: (birth, 0),
        zombie: state == "Z",
    })
}

#[cfg(target_os = "linux")]
fn identity(pid: i32) -> io::Result<Option<Process>> {
    use std::{io::Read, os::unix::fs::MetadataExt};
    if pid <= 0 {
        return Err(invalid("PID must be positive"));
    }
    finish((|| {
        let directory = std::path::PathBuf::from(format!("/proc/{pid}"));
        let uid = std::fs::metadata(&directory)?.uid();
        let mut bytes = Vec::new();
        std::fs::File::open(directory.join("stat"))?
            .take(MAX_STAT as u64 + 1)
            .read_to_end(&mut bytes)?;
        let process = parse_stat(&bytes, uid)?;
        if process.pid != pid {
            return Err(invalid("mismatched stat PID"));
        }
        Ok(process)
    })())
}

#[cfg(target_os = "macos")]
fn identity(pid: i32) -> io::Result<Option<Process>> {
    if pid <= 0 {
        return Err(invalid("PID must be positive"));
    }
    finish((|| {
        // SAFETY: exact initialized kernel output structure and size.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info) as i32;
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        if read == 0 {
            return Err(io::Error::last_os_error());
        }
        if read != size || info.pbi_pid != pid as u32 {
            return Err(invalid("incomplete process identity"));
        }
        let sid = unsafe { libc::getsid(pid) };
        if sid < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Process {
            pid,
            parent: info.pbi_ppid as i32,
            sid,
            uid: info.pbi_uid,
            birth: (info.pbi_start_tvsec, info.pbi_start_tvusec),
            zombie: info.pbi_status == libc::SZOMB,
        })
    })())
}

#[cfg(target_os = "linux")]
fn pids() -> io::Result<Vec<i32>> {
    let mut pids = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        if let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
            .filter(|pid| *pid > 0)
        {
            if pids.len() == MAX_SCANNED {
                return Err(invalid("process enumeration exceeds bound"));
            }
            pids.push(pid);
        }
    }
    Ok(pids)
}

#[cfg(target_os = "macos")]
fn pids() -> io::Result<Vec<i32>> {
    let mut pids = vec![0_i32; MAX_SCANNED + 1];
    // SAFETY: initialized writable bounded output array.
    let count = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * std::mem::size_of::<i32>()) as i32,
        )
    };
    if count <= 0 || count as usize > MAX_SCANNED {
        return Err(invalid("failed/excessive process enumeration"));
    }
    pids.truncate(count as usize);
    pids.retain(|pid| *pid > 0);
    Ok(pids)
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
