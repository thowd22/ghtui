//! Closed optional renderer kernel. Called only inside the existing original
//! native worker/request. Byte inputs grant NO canonical/read/write authority.
use crate::{
    native_diff_operation_v3::NativeDiffCheckV3,
    providers::{
        local_git_navigation_read_v1::LocalGitNavigationReadV1,
        owned_pty_viewer_v4::{
            DisplayPermitV4, PagerProbeV4, PreparedPagerCaptureV4, VerifiedPagerV4,
        },
        runner::{CancellationToken, CommandOutput, CommandRunner, CommandSpec, FailureKind},
    },
};
use std::{
    ffi::OsStr,
    fs::OpenOptions,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_RETAINED: usize = 8 * 1024 * 1024;
const MAX_TRANSIENT: usize = 16 * 1024 * 1024;
const RENDER_CAPTURE: usize = 512 * 1024;
const MAX_PATCH: usize = 2 * 1024 * 1024;
const MAX_SIDE: usize = 2 * 1024 * 1024;
const ACCOUNTING_RESERVE: usize = 1024 * 1024;
pub(crate) fn language_suffix_v4(path: &crate::git::RawPath) -> &str {
    let Some(dot) = path.0.iter().rposition(|byte| *byte == b'.') else {
        return "txt";
    };
    let Ok(suffix) = std::str::from_utf8(&path.0[dot + 1..]) else {
        return "txt";
    };
    match suffix {
        "rs" | "go" | "py" | "c" | "cc" | "cpp" | "h" | "hpp" | "js" | "ts" | "tsx" | "jsx"
        | "json" | "yaml" | "yml" | "toml" | "md" | "txt" | "sh" | "sql" | "java" | "kt"
        | "swift" | "rb" | "css" | "html" | "xml" => suffix,
        _ => "txt",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExternalToolV4 {
    Delta,
    Difftastic,
}
impl ExternalToolV4 {
    pub(crate) fn program(self) -> &'static str {
        match self {
            Self::Delta => "delta",
            Self::Difftastic => "difft",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Delta => "delta",
            Self::Difftastic => "difftastic",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExternalPatchScopeV4 {
    LocalCapturedComparison,
    PrSelectedFile,
}
impl ExternalPatchScopeV4 {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::LocalCapturedComparison => {
                "WHOLE ORIGINAL CAPTURED COMPARISON • not selected-file scope"
            }
            Self::PrSelectedFile => "ONE VERIFIED SELECTED PR FILE • M → H • not the whole PR",
        }
    }
}
/// Opaque borrow from one genuine sealed source. No public byte/role/scope
/// constructor exists, and this value cannot produce canonical coordinates.
#[doc(hidden)]
pub struct NativeExternalPatchV4<'a> {
    bytes: &'a [u8],
    scope: ExternalPatchScopeV4,
}
impl<'a> NativeExternalPatchV4<'a> {
    pub(crate) fn saved(source: &'a crate::git::OriginalComparisonSourceV1) -> Self {
        Self {
            bytes: &source.result().patch,
            scope: ExternalPatchScopeV4::LocalCapturedComparison,
        }
    }
    pub(crate) fn mutable(source: &'a crate::git::MutableDiffSourceV2) -> Self {
        Self {
            bytes: &source.result().patch,
            scope: ExternalPatchScopeV4::LocalCapturedComparison,
        }
    }
    pub(crate) fn pr(
        source: &'a crate::scoped_pr_native_diff_source_v1::ScopedPrNativeSourceV1,
    ) -> Self {
        Self {
            bytes: source.proof().observed().selected_raw_patch_v4(),
            scope: ExternalPatchScopeV4::PrSelectedFile,
        }
    }
    pub(crate) fn input(&self) -> ExternalOriginalInputV4<'_> {
        ExternalOriginalInputV4::Patch {
            bytes: self.bytes,
            scope: self.scope,
        }
    }
    pub(crate) fn scope_label(&self) -> &'static str {
        self.scope.label()
    }
}
/// Borrowed only from the genuine checked source/side factories in the native
/// request. This is not constructible from a displayed row or outside crate.
pub(crate) enum ExternalOriginalInputV4<'a> {
    Patch {
        bytes: &'a [u8],
        scope: ExternalPatchScopeV4,
    },
    TextSides {
        old: &'a [u8],
        new: &'a [u8],
        old_absent: bool,
        new_absent: bool,
        suffix: &'a str,
        new_material_bytes: usize,
    },
}
impl ExternalOriginalInputV4<'_> {
    fn validate(&self, tool: ExternalToolV4) -> Result<usize, String> {
        match (self, tool) {
            (Self::Patch { bytes, scope }, ExternalToolV4::Delta) => {
                let cap = if *scope == ExternalPatchScopeV4::PrSelectedFile {
                    1024 * 1024
                } else {
                    MAX_PATCH
                };
                if bytes.len() > cap {
                    return Err("Original canonical patch exceeds optional external input cap; native retained".into());
                }
                Ok(bytes.len())
            }
            (
                Self::TextSides {
                    old,
                    new,
                    old_absent,
                    new_absent,
                    suffix,
                    new_material_bytes,
                },
                ExternalToolV4::Difftastic,
            ) => {
                if old.len() > MAX_SIDE
                    || new.len() > MAX_SIDE
                    || *new_material_bytes > MAX_RETAINED
                    || (*old_absent && !old.is_empty())
                    || (*new_absent && !new.is_empty())
                {
                    return Err("Original text-side evidence exceeds optional bounds or contradicts absence".into());
                }
                if suffix.len() > 12 || !suffix.bytes().all(|b| b.is_ascii_alphanumeric()) {
                    return Err("External language suffix must be bounded generated ASCII, never a raw path".into());
                }
                Ok(*new_material_bytes)
            }
            _ => Err(
                "External tool/input protocol mismatch; no alternate command or source inferred"
                    .into(),
            ),
        }
    }
}

/// Bounds only. Local constructor borrows the SAME genuine read operation and
/// actual runner, but does NOT pass foreign programs to its Git-only window.
pub(crate) struct ExternalRendererBoundsV4<'a, O: NativeDiffCheckV3> {
    operation: &'a O,
    runner: CommandRunner,
    deadline: Instant,
    original: CancellationToken,
    display: Arc<DisplayPermitV4>,
    #[cfg(test)]
    programs_override: Option<(PathBuf, PathBuf)>,
}
impl<'a> ExternalRendererBoundsV4<'a, LocalGitNavigationReadV1> {
    pub(crate) fn local(
        operation: &'a LocalGitNavigationReadV1,
        display: Arc<DisplayPermitV4>,
    ) -> Result<Self, String> {
        let window = operation.window();
        Self::original(
            operation,
            window.actual_runner_v1().clone(),
            operation.deadline(),
            operation.cancellation().clone(),
            display,
        )
    }
}
impl<'a, O: NativeDiffCheckV3> ExternalRendererBoundsV4<'a, O> {
    pub(crate) fn matches_original(&self, operation: &O, display: &Arc<DisplayPermitV4>) -> bool {
        std::ptr::eq(self.operation, operation) && Arc::ptr_eq(&self.display, display)
    }
    /// Remote caller is the original PR worker, passing its actual provider's
    /// runner and producing check-only operation/token/deadline. No auth/API
    /// lease is stored here. Exact construction is reviewed at that call site.
    pub(crate) fn original(
        operation: &'a O,
        runner: CommandRunner,
        deadline: Instant,
        original: CancellationToken,
        display: Arc<DisplayPermitV4>,
    ) -> Result<Self, String> {
        if deadline
            .checked_duration_since(Instant::now())
            .is_none_or(|left| left.is_zero() || left > Duration::from_secs(30))
        {
            return Err(
                "External preparation requires original <=30-second source deadline".into(),
            );
        }
        let bounds = Self {
            operation,
            runner,
            deadline,
            original,
            display,
            #[cfg(test)]
            programs_override: None,
        };
        bounds.check()?;
        Ok(bounds)
    }
    fn programs(&self, tool: ExternalToolV4) -> Result<(PathBuf, PathBuf), String> {
        #[cfg(test)]
        if let Some(programs) = &self.programs_override {
            return Ok(programs.clone());
        }
        Ok((resolve(tool.program())?, resolve("less")?))
    }
    fn check(&self) -> Result<(), String> {
        self.operation.check()?;
        if self.original.is_cancelled()
            || self.display.cancellation().is_cancelled()
            || Instant::now() >= self.deadline
            || Instant::now() >= self.display.deadline()
        {
            Err("Original optional renderer cancelled/expired; no source or clock renewal".into())
        } else {
            Ok(())
        }
    }
    fn execute(&self, mut spec: CommandSpec, maximum: Duration) -> Result<CommandOutput, String> {
        self.check()?;
        spec.timeout = maximum.min(self.deadline.saturating_duration_since(Instant::now()));
        spec.absolute_deadline_v1 = Some(self.deadline.min(Instant::now() + maximum));
        spec.original_read_cancellation = Some(self.original.clone());
        spec.coalesce_read = false;
        // Scoped read budget/window intentionally NONE. It remains installed-
        // git-only; the normal runner receives only this fixed renderer spec.
        let result=self.runner.run(spec,self.display.cancellation()).map_err(|error|match error.kind{
            FailureKind::MissingProgram=>"Optional external tool unavailable; native unchanged".to_owned(),
            FailureKind::Timeout=>"Optional external tool exceeded original deadline; native unchanged".to_owned(),
            FailureKind::Cancelled=>"Optional external tool cancelled; native unchanged".to_owned(),
            FailureKind::OutputLimit=>"Optional renderer512KiB combined output cap reached; native unchanged, partial output NOT complete".to_owned(),
            FailureKind::Exit=>format!("Optional external tool exited{}; native unchanged",error.exit_code.unwrap_or(-1)),
            _=>"Optional external tool failed; native unchanged".to_owned(),
        })?;
        self.check()?;
        Ok(result)
    }
}

#[derive(PartialEq, Eq)]
struct ExecutableStamp {
    dev: u64,
    ino: u64,
    mode: u32,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}
fn stamp(path: &Path) -> Result<ExecutableStamp, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "Optional executable unavailable")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.mode() & 0o111 == 0 {
        return Err("Optional executable is not a regular resolved program".into());
    }
    Ok(ExecutableStamp {
        dev: metadata.dev(),
        ino: metadata.ino(),
        mode: metadata.mode(),
        len: metadata.len(),
        mtime: (metadata.mtime(), metadata.mtime_nsec()),
        ctime: (metadata.ctime(), metadata.ctime_nsec()),
    })
}
fn resolve(program: &str) -> Result<PathBuf, String> {
    let paths = std::env::var_os("PATH").ok_or("No PATH for optional external program")?;
    if paths.as_encoded_bytes().len() > 32768 {
        return Err("External PATH exceeds bounded lookup".into());
    }
    for (index, directory) in std::env::split_paths(&paths).enumerate() {
        if index == 128 {
            return Err("External PATH entry bound reached".into());
        }
        if !directory.is_absolute() {
            continue;
        }
        let candidate = directory.join(program);
        if let Ok(path) = std::fs::canonicalize(candidate) {
            if stamp(&path).is_ok() {
                return Ok(path);
            }
        }
    }
    Err(format!(
        "Optional {program} not installed; native remains available"
    ))
}
fn clean_spec(program: &Path, cwd: &Path) -> CommandSpec {
    let mut spec = CommandSpec::new(program.as_os_str()).cwd(cwd).fresh_read();
    spec.clear_env = true;
    for (key, value) in [
        ("PATH", "/usr/bin:/bin"),
        ("TERM", "xterm-256color"),
        ("LANG", "C"),
        ("SHELL", "/bin/false"),
        ("PAGER", "cat"),
        ("GIT_PAGER", "cat"),
        ("LESSSECURE", "1"),
        ("LESSSECURE_ALLOW", ""),
    ] {
        spec = spec.env(key, value);
    }
    spec = spec
        .env("HOME", cwd.as_os_str())
        .env("XDG_CONFIG_HOME", cwd.as_os_str())
        .env("TMPDIR", cwd.as_os_str());
    spec
}
fn write_side<O: NativeDiffCheckV3>(
    bounds: &ExternalRendererBoundsV4<'_, O>,
    directory: &Path,
    name: &OsStr,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    bounds.check()?;
    let path = directory.join(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|_| "Could not create private external snapshot")?;
    let metadata = file
        .metadata()
        .map_err(|_| "Private external snapshot unavailable")?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.nlink() != 1 || !metadata.is_file()
    {
        return Err("External snapshot is not private regular owned file".into());
    }
    bounds.check()?;
    file.write_all(bytes)
        .map_err(|_| "Could not write bounded external snapshot")?;
    file.flush()
        .map_err(|_| "Could not flush external snapshot")?;
    file.set_permissions(std::fs::Permissions::from_mode(0o400))
        .map_err(|_| "Could not protect external snapshot")?;
    bounds.check()?;
    Ok(path)
}

pub(crate) struct PreparedNativeExternalV4 {
    pub(crate) capture: PreparedPagerCaptureV4,
    pub(crate) pager: VerifiedPagerV4,
    pub(crate) display: Arc<DisplayPermitV4>,
    pub(crate) tool: ExternalToolV4,
    pub(crate) width: u16,
    pub(crate) scope: Arc<str>,
}
pub(crate) fn prepare_local_v4<S>(
    request: &Arc<crate::repository_native_diff_v1::NativeDiffRequestV1<S>>,
    operation: &LocalGitNavigationReadV1,
) -> Result<crate::repository_native_diff_v1::PreparedNativeDiffV1<S>, String>
where
    S: crate::repository_native_diff_content_v2::NativeDiffPurposeV2<
        Operation = LocalGitNavigationReadV1,
    >,
{
    match request.external_display_v4() {
        Some(display) => {
            let bounds = ExternalRendererBoundsV4::local(operation, display.clone())?;
            request.prepare_external_v4(operation, &bounds)
        }
        None => request.prepare(operation),
    }
}
impl PreparedNativeExternalV4 {
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.scope.len() + self.capture.prepared_charge() + 4096
    }
}

/// ONE checked producer calls this with genuine original patch/side borrows and
/// its exact source-validation closure. No output is decoded as Git/review data.
pub(crate) fn prepare_external_v4<O: NativeDiffCheckV3>(
    bounds: &ExternalRendererBoundsV4<'_, O>,
    tool: ExternalToolV4,
    input: ExternalOriginalInputV4<'_>,
    width: u16,
    validate: impl Fn() -> Result<(), String>,
) -> Result<PreparedNativeExternalV4, String> {
    bounds.check()?;
    validate()?;
    if !(12..=240).contains(&width) {
        return Err("External renderer requires actual bounded12..240-column pane".into());
    }
    let material_bytes = input.validate(tool)?;
    // This initial narrow cap bounds the existing runner's lossy/sanitize/
    // redaction temporaries on FAILURE too, not just successful raw stdout.
    if material_bytes
        .checked_add(16 * RENDER_CAPTURE + ACCOUNTING_RESERVE)
        .is_none_or(|n| n > MAX_TRANSIENT)
    {
        return Err("Optional renderer transient16MiB preflight refused; native retained".into());
    }
    let directory = tempfile::Builder::new()
        .prefix("ghtui-render-")
        .tempdir()
        .map_err(|_| "Could not create private renderer directory")?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "Could not protect renderer directory")?;
    let (renderer, pager_path) = bounds.programs(tool)?;
    let renderer_stamp = stamp(&renderer)?;
    bounds.check()?;
    validate()?;
    let mut probe = clean_spec(&renderer, directory.path()).args(["--version"]);
    probe.output_limit = 4096;
    let version = bounds.execute(probe, Duration::from_secs(1))?;
    let first = std::str::from_utf8(&version.stdout)
        .ok()
        .and_then(|s| s.lines().next())
        .unwrap_or("");
    let mut words = first.split_ascii_whitespace();
    let expected = if tool == ExternalToolV4::Delta {
        "delta"
    } else {
        "difftastic"
    };
    if words
        .next()
        .is_none_or(|word| !word.eq_ignore_ascii_case(expected))
        || words
            .next()
            .is_none_or(|version| !version.as_bytes().first().is_some_and(u8::is_ascii_digit))
    {
        return Err("Unrecognized optional renderer version; no unsafe protocol downgrade".into());
    }
    let pager_probe = PagerProbeV4::capture(pager_path)
        .map_err(|_| "Optional patched less704+ unavailable; native retained")?;
    let mut probe = clean_spec(pager_probe.program(), directory.path()).args(["--version"]);
    probe.output_limit = 4096;
    let version = bounds.execute(probe, Duration::from_secs(1))?;
    let pager = pager_probe.verify(&version.stdout).map_err(|_| {
        "Optional patched less704+ unavailable or changed during probe; native retained"
    })?;
    bounds.check()?;
    validate()?;
    let mut spec = clean_spec(&renderer, directory.path());
    let scope = match input {
        ExternalOriginalInputV4::Patch { bytes, scope } => {
            spec = spec
                .args([
                    "--no-gitconfig",
                    "--paging=never",
                    "--detect-dark-light=never",
                    &format!("--width={width}"),
                    "--line-numbers",
                ])
                .input(bytes.to_vec());
            Arc::<str>::from(format!(
                "delta • {} • output cap512KiB • native remains available",
                scope.label()
            ))
        }
        ExternalOriginalInputV4::TextSides {
            old,
            new,
            old_absent,
            new_absent,
            suffix,
            ..
        } => {
            let extension = if suffix.is_empty() { "txt" } else { suffix };
            let old_path = write_side(
                bounds,
                directory.path(),
                OsStr::new(&format!("old.{extension}")),
                old,
            )?;
            let new_path = write_side(
                bounds,
                directory.path(),
                OsStr::new(&format!("new.{extension}")),
                new,
            )?;
            spec = spec
                .args([
                    "--display=side-by-side-show-both",
                    "--color=always",
                    &format!("--width={width}"),
                    "--context=3",
                    "--byte-limit=1048576",
                    "--graph-limit=250000",
                    "--parse-error-limit=0",
                    "--strip-cr=off",
                    "--",
                ])
                .args([old_path.as_os_str(), new_path.as_os_str()]);
            Arc::<str>::from(format!(
                "difftastic • selected genuine full sides • Old{} / New{} • output cap512KiB",
                if old_absent {
                    " ABSENT (temporary empty input, not blob)"
                } else {
                    " observed bytes"
                },
                if new_absent {
                    " ABSENT (temporary empty input, not blob)"
                } else {
                    " observed bytes"
                }
            ))
        }
    };
    spec.output_limit = RENDER_CAPTURE;
    bounds.check()?;
    validate()?;
    if stamp(&renderer)? != renderer_stamp {
        return Err("Optional renderer changed after original version probe".into());
    }
    let output = bounds.execute(spec, Duration::from_secs(10))?;
    let raw_capacity = output
        .stdout
        .capacity()
        .saturating_add(output.stderr.capacity());
    if material_bytes
        .checked_add(raw_capacity)
        .and_then(|n| n.checked_add(4 * 1024 * 1024 + ACCOUNTING_RESERVE))
        .is_none_or(|n| n > MAX_TRANSIENT)
    {
        return Err("External actual capture capacities exceed16MiB transient bound".into());
    }
    bounds.check()?;
    validate()?;
    let capture = PreparedPagerCaptureV4::from_renderer(&output.stdout)
        .map_err(|_| "Optional output failed bounded terminal sanitization; native retained")?;
    drop(output);
    // Inputs/material borrows and obsolete raw capture are not retained in the
    // prepared result. Real temp close failure is not a successful handoff.
    directory
        .close()
        .map_err(|_| "External original-input temporary cleanup incomplete; native retained")?;
    bounds.check()?;
    validate()?;
    let result = PreparedNativeExternalV4 {
        capture,
        pager,
        display: bounds.display.clone(),
        tool,
        width,
        scope,
    };
    if result.retained_bytes() > MAX_RETAINED {
        return Err("External prepared8MiB bound exceeded".into());
    }
    bounds.check()?;
    Ok(result)
}

#[cfg(test)]
#[path = "repository_native_external_v4_tests.rs"]
mod tests;
