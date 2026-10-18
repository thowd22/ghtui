//! TEST ONLY real HTTPS smart-Git fixture. No product TLS/auth/client or
//! endpoint override. Consumers configure this disposable repository BEFORE
//! capturing its genuine original source/config, never after approval.
use super::owned_pty_processes_v1 as owned;
use std::{
    fs,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        io::AsRawFd,
    },
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[cfg(test)]
#[path = "workflow_tls_v9/tests.rs"]
pub mod contract_hooks;
#[path = "workflow_tls_v9/process.rs"]
mod process;
#[path = "workflow_tls_v9/protocol.rs"]
mod protocol;
#[path = "workflow_tls_v9/service.rs"]
mod service;

const URL: &str = "https://ghe.example/example/ci.git";
const EXCHANGES: usize = 16;

/// Fixed descriptive metadata only. In particular no request/header/body,
/// authorization/cookies, pack, URL credential, private path or key material.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RequestMetadata {
    pub method: &'static str,
    pub path: &'static str,
    pub status: u16,
    pub request_bytes: usize,
    pub response_bytes: usize,
}
#[derive(Debug, serde::Serialize)]
pub struct CleanupReport {
    pub exchanges: usize,
    pub requests: Vec<RequestMetadata>,
    pub processes: Vec<owned::CleanupReport>,
    pub io_threads_joined: usize,
}

struct Shared {
    stop: AtomicBool,
    failed: AtomicBool,
    deadline: Instant,
    exchanges: AtomicUsize,
    joined: AtomicUsize,
    requests: Mutex<Vec<RequestMetadata>>,
    processes: Arc<Mutex<Vec<owned::CleanupReport>>>,
}
/// Until `Self` exists, setup failures must obey the same file-retention
/// invariant as shutdown. Reports are descriptive cleanup evidence only.
struct StartDirectory {
    directory: Option<tempfile::TempDir>,
    reports: Arc<Mutex<Vec<owned::CleanupReport>>>,
}
impl Drop for StartDirectory {
    fn drop(&mut self) {
        let unproven = match self.reports.lock() {
            Ok(reports) => reports.iter().any(|report| {
                report.application_pid <= 0
                    || !report.remaining.is_empty()
                    || !report.errors.is_empty()
            }),
            Err(_) => true,
        };
        if unproven {
            if let Some(directory) = self.directory.take() {
                let _ = directory.keep();
            }
        }
    }
}
pub struct WorkflowTlsRemoteV9 {
    directory: Option<tempfile::TempDir>,
    home: PathBuf,
    bare: PathBuf,
    ca: PathBuf,
    proxy: SocketAddr,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<Result<(), String>>>,
    git: PathBuf,
    configured: AtomicBool,
}
impl std::fmt::Debug for WorkflowTlsRemoteV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "WorkflowTlsRemoteV9(<owned offline TLS/real bare Git; private paths redacted>)",
        )
    }
}

fn private_dir(path: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|_| "Fixture directory is unavailable")?;
    let meta = fs::symlink_metadata(path).map_err(|_| "Fixture directory metadata unavailable")?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
    {
        return Err("TLS fixture root must be an actual owned private directory".into());
    }
    Ok(canonical)
}
fn executable(name: &str) -> Result<PathBuf, String> {
    for dir in std::env::split_paths(
        &std::env::var_os("PATH").ok_or("Installed fixture tool PATH unavailable")?,
    ) {
        let candidate = dir.join(name);
        let Ok(meta) = fs::metadata(&candidate) else {
            continue;
        };
        if meta.is_file() && meta.permissions().mode() & 0o111 != 0 {
            return fs::canonicalize(candidate)
                .map_err(|_| "Installed fixture executable unavailable".into());
        }
    }
    Err("Required installed Git/OpenSSL fixture tool unavailable; no skip/fallback".into())
}
fn git_command(git: &Path, repo: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(git);
    command.arg("-C").arg(repo).args(args);
    command
}

impl WorkflowTlsRemoteV9 {
    pub fn start(private_fixture_root: &Path, real_seed_repo: &Path) -> Result<Self, String> {
        let root = private_dir(private_fixture_root)?;
        let seed = fs::canonicalize(real_seed_repo)
            .map_err(|_| "Actual fixture seed repository unavailable")?;
        if !seed.starts_with(&root)
            || !fs::symlink_metadata(real_seed_repo)
                .map_err(|_| "Seed metadata unavailable")?
                .is_dir()
        {
            return Err("Actual seed must remain inside this private disposable fixture".into());
        }
        let git = executable("git")?;
        let openssl = executable("openssl")?;
        let directory = tempfile::Builder::new()
            .prefix("workflow-tls-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&root)
            .map_err(|_| "Private TLS fixture directory creation failed")?;
        let home = directory.path().join("home");
        let gitroot = directory.path().join("gitroot");
        let ca = directory.path().join("fixture-ca.pem");
        let key = directory.path().join("fixture-key.pem");
        let reports = Arc::new(Mutex::new(Vec::new()));
        let mut start_directory = StartDirectory {
            directory: Some(directory),
            reports: reports.clone(),
        };
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&home)
            .map_err(|_| "Private fixture HOME creation failed")?;
        fs::create_dir_all(gitroot.join("example"))
            .map_err(|_| "Private Git root creation failed")?;
        let bare = gitroot.join("example/ci.git");
        let mut clone = Command::new(&git);
        clone
            .args(["clone", "--bare", "--no-hardlinks", "--no-local", "--"])
            .arg(&seed)
            .arg(&bare);
        process::run(clone, &home, &[], 64 * 1024, &reports)?;
        process::run(
            git_command(
                &git,
                &bare,
                &["config", "--local", "http.receivepack", "true"],
            ),
            &home,
            &[],
            64 * 1024,
            &reports,
        )?;
        let mut exec = Command::new(&git);
        exec.arg("--exec-path");
        let output = process::run(exec, &home, &[], 16 * 1024, &reports)?;
        let exec_path = std::str::from_utf8(&output)
            .map_err(|_| "Installed Git exec path is not UTF8")?
            .trim_end_matches('\n');
        if exec_path.is_empty() || exec_path.contains(['\n', '\r', '\0']) {
            return Err("Installed Git exec path malformed".into());
        }
        let backend = fs::canonicalize(Path::new(exec_path).join("git-http-backend"))
            .map_err(|_| "Installed git-http-backend unavailable")?;
        if !fs::metadata(&backend)
            .map_err(|_| "Git backend metadata unavailable")?
            .is_file()
        {
            return Err("Actual Git backend is not a regular executable".into());
        }
        for path in [&ca, &key] {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)
                .map_err(|_| "Private fixture certificate file creation failed")?;
        }
        let mut certificate = Command::new(&openssl);
        certificate
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=ghe.example",
                "-addext",
                "subjectAltName=DNS:ghe.example",
                "-addext",
                "basicConstraints=critical,CA:TRUE",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&ca);
        process::run(certificate, &home, &[], 64 * 1024, &reports)?;
        for path in [&ca, &key] {
            let meta = fs::symlink_metadata(path)
                .map_err(|_| "Private certificate metadata unavailable")?;
            if !meta.is_file()
                || meta.uid() != unsafe { libc::geteuid() }
                || meta.nlink() != 1
                || meta.mode() & 0o077 != 0
                || meta.len() == 0
                || meta.len() > 16 * 1024
            {
                return Err("Private bounded certificate/key invariant failed".into());
            }
        }
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| "Owned CONNECT listener failed")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Owned CONNECT listener flags failed")?;
        let proxy = listener
            .local_addr()
            .map_err(|_| "Owned CONNECT address unavailable")?;
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            // Whole push+PR journeys need headroom on slow hosted CI runners;
            // an expired fixture looks like a fenced scope, not a timeout.
            deadline: Instant::now() + Duration::from_secs(180),
            exchanges: AtomicUsize::new(0),
            joined: AtomicUsize::new(0),
            requests: Mutex::new(Vec::with_capacity(EXCHANGES)),
            processes: reports,
        });
        let service = service::Service {
            openssl,
            backend,
            home: home.clone(),
            ca: ca.clone(),
            key,
            gitroot,
        };
        let state = shared.clone();
        let worker = thread::Builder::new()
            .name("w9-owned-tls".into())
            .spawn(move || {
                // Surface the reason in captured test output; otherwise a
                // stopped fixture only shows up as a fenced scope on screen.
                let result = service.run(listener, &state);
                if let Err(error) = &result {
                    eprintln!("workflow TLS fixture stopped: {error}");
                }
                result
            })
            .map_err(|_| "Owned TLS fixture worker failed to spawn")?;
        Ok(Self {
            directory: start_directory.directory.take(),
            home,
            bare,
            ca,
            proxy,
            shared,
            worker: Some(worker),
            git,
            configured: AtomicBool::new(false),
        })
    }
    /// Setup-only: no Source has been captured yet. The canonical advertised
    /// URL remains untouched; only this actual fixture's local config changes.
    pub fn configure_fixture_repository(
        &self,
        repo: &Path,
        remote_name: &str,
    ) -> Result<(), String> {
        process::alive(&self.shared.stop, self.shared.deadline)?;
        let directory = self
            .directory
            .as_ref()
            .ok_or("TLS fixture already finished")?;
        let fixture_root = directory
            .path()
            .parent()
            .ok_or("Fixture parent unavailable")?;
        let canonical =
            fs::canonicalize(repo).map_err(|_| "Actual fixture Git repository unavailable")?;
        if !canonical.starts_with(fixture_root)
            || remote_name.is_empty()
            || remote_name.len() > 256
            || remote_name.starts_with('-')
            || !remote_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err("Fixture-only configured repository/remote refused".into());
        }
        if self.configured.swap(true, Ordering::AcqRel) {
            return Err(
                "Fixture repository configuration is one-shot before Source capture".into(),
            );
        }
        let pairs = [
            (format!("remote.{remote_name}.url"), URL.to_owned()),
            (
                format!("remote.{remote_name}.fetch"),
                format!("+refs/heads/*:refs/remotes/{remote_name}/*"),
            ),
            (
                "http.https://ghe.example/.proxy".into(),
                format!("http://{}", self.proxy),
            ),
            (
                "http.https://ghe.example/.sslCAInfo".into(),
                self.ca
                    .to_str()
                    .ok_or("Fixture CA path is not UTF8")?
                    .into(),
            ),
        ];
        for (key, value) in pairs {
            process::run(
                git_command(
                    &self.git,
                    &canonical,
                    &["config", "--local", "--replace-all", &key, &value],
                ),
                &self.home,
                &[],
                16 * 1024,
                &self.shared.processes,
            )?;
        }
        Ok(())
    }
    pub fn canonical_url(&self) -> &'static str {
        URL
    }
    pub fn bare_repository(&self) -> &Path {
        &self.bare
    }
    pub fn requests(&self) -> Result<Vec<RequestMetadata>, String> {
        let requests = self
            .shared
            .requests
            .lock()
            .map_err(|_| "Fixture request metadata poisoned")?;
        if requests.len() > EXCHANGES {
            return Err("Fixture request metadata bound exceeded".into());
        }
        Ok(requests.clone())
    }
    pub fn finish(mut self) -> Result<CleanupReport, String> {
        self.shutdown()
    }
    fn shutdown(&mut self) -> Result<CleanupReport, String> {
        let result = self.shutdown_inner();
        // If exact cleanup cannot be proven, preserve private files instead
        // of unlinking a key/repository underneath a possibly live child.
        if result.is_err() {
            if let Some(directory) = self.directory.take() {
                let _ = directory.keep();
            }
        }
        result
    }
    fn shutdown_inner(&mut self) -> Result<CleanupReport, String> {
        self.shared.stop.store(true, Ordering::Release);
        let until = Instant::now() + Duration::from_secs(8);
        if let Some(worker) = self.worker.take() {
            while !worker.is_finished() && Instant::now() < until {
                thread::sleep(Duration::from_millis(2));
            }
            if !worker.is_finished() {
                if let Some(directory) = self.directory.take() {
                    let _ = directory.keep();
                }
                return Err("Owned TLS fixture worker did not finish in bounded cleanup; private directory retained".into());
            }
            self.shared.joined.fetch_add(1, Ordering::AcqRel);
            worker
                .join()
                .map_err(|_| "Owned TLS fixture worker panicked")??;
        }
        let processes = std::mem::take(
            &mut *self
                .shared
                .processes
                .lock()
                .map_err(|_| "Fixture cleanup metadata poisoned")?,
        );
        if self.shared.failed.load(Ordering::Acquire) {
            return Err(
                "TLS fixture exceeded a bound or failed an owned I/O/cleanup contract".into(),
            );
        }
        if processes
            .iter()
            .any(|r| r.application_pid <= 0 || !r.remaining.is_empty() || !r.errors.is_empty())
        {
            return Err("Fixture process absence/cleanup was not proven".into());
        }
        Ok(CleanupReport {
            exchanges: self.shared.exchanges.load(Ordering::Acquire),
            requests: self.requests()?,
            processes,
            io_threads_joined: self.shared.joined.load(Ordering::Acquire),
        })
    }
}
impl Drop for WorkflowTlsRemoteV9 {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
