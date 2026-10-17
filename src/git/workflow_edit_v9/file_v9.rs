//! Descriptor-bound regular working-file observation for explicit W9 editing.
//! Neither bytes nor this stamp is a Save/Stage/GitHub permission.
use super::*;
use std::{
    ffi::CString,
    fs::File,
    io::Write,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::fs::MetadataExt,
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(PartialEq, Eq)]
struct LeafV9 {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    links: u64,
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
static SAVE_SERIAL_V9: AtomicU64 = AtomicU64::new(1);
#[cfg(test)]
type BeforeSaveRenameObserverV9 = Box<dyn FnOnce(Instant)>;
#[cfg(test)]
thread_local! {
    // Observation only: this hook receives the actual closed release clock,
    // never changes it or bypasses any production identity/clock decision.
    static BEFORE_SAVE_RENAME_V9: std::cell::RefCell<Option<BeforeSaveRenameObserverV9>> =
        const { std::cell::RefCell::new(None) };
}
struct SaveTemporaryV9<'a> {
    parent: &'a ParentV9,
    name: CString,
    file: File,
    renamed: bool,
}
impl Drop for SaveTemporaryV9<'_> {
    fn drop(&mut self) {
        if self.renamed {
            return;
        }
        // Cleanup only our exact created descriptor/name, never a broad path.
        // A replacement/cleanup error remains an inspect-required outcome.
        let mut current = std::mem::MaybeUninit::<libc::stat>::uninit();
        if let Ok(meta) = self.file.metadata() {
            if unsafe {
                libc::fstatat(
                    self.parent.directory.as_raw_fd(),
                    self.name.as_ptr(),
                    current.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            } == 0
            {
                let current = unsafe { current.assume_init() };
                #[cfg(target_os = "linux")]
                let device_matches = current.st_dev == meta.dev();
                #[cfg(target_os = "macos")]
                let device_matches = current.st_dev as u64 == meta.dev();
                if device_matches && current.st_ino == meta.ino() {
                    unsafe {
                        libc::unlinkat(self.parent.directory.as_raw_fd(), self.name.as_ptr(), 0);
                    }
                }
            }
        }
    }
}
impl LeafV9 {
    fn of(m: &std::fs::Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            mode: m.mode(),
            uid: m.uid(),
            links: m.nlink(),
            bytes: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
}
struct ParentV9 {
    directory: File,
    name: CString,
    chain: Vec<DirectoryV9>,
}
#[derive(PartialEq, Eq)]
struct DirectoryV9 {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
}
impl DirectoryV9 {
    fn of(m: &std::fs::Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            mode: m.mode(),
            uid: m.uid(),
        }
    }
}
pub(crate) struct WorkflowWorkingFileV9 {
    binding: Arc<WorkflowLocalBindingV9>,
    leaf: LeafV9,
    parents: Vec<DirectoryV9>,
    content: Arc<[u8]>,
}
impl fmt::Debug for WorkflowWorkingFileV9 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkflowWorkingFileV9(<actual descriptor bytes; no approval>)")
    }
}
fn fd(result: libc::c_int) -> GitResult<File> {
    if result < 0 {
        return Err(unavailable());
    }
    // SAFETY: successful open/openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(result) })
}
fn cstring(bytes: &[u8]) -> GitResult<CString> {
    CString::new(bytes).map_err(|_| unavailable())
}
pub(super) fn root_descriptor_v9(origin: &OriginalLocalActorOriginV1) -> GitResult<File> {
    let source = origin.source();
    let root = fd(unsafe {
        libc::open(
            cstring(source.root().as_os_str().as_bytes())?.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    })?;
    let meta = root.metadata()?;
    if !meta.is_dir() || (meta.dev(), meta.ino()) != source.root_file_identity_v3() {
        return Err(unavailable());
    }
    Ok(root)
}
fn check(
    binding: &WorkflowLocalBindingV9,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GitResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(unavailable());
    }
    binding.check_origin_v9(cancel)
}
fn parent(
    binding: &WorkflowLocalBindingV9,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GitResult<ParentV9> {
    check(binding, cancel, deadline)?;
    let source = binding.origin.source();
    let raw = binding.intent.definition().workflow.path.as_bytes();
    if !crate::scoped_actions_workflow_edit_v9::safe_workflow_path_v9(
        &binding.intent.definition().workflow.path,
    ) {
        return Err(unavailable());
    }
    let root = root_descriptor_v9(&binding.origin)?;
    let meta = root.metadata()?;
    if !meta.is_dir() || (meta.dev(), meta.ino()) != source.root_file_identity_v3() {
        return Err(unavailable());
    }
    let parts = raw.split(|byte| *byte == b'/').collect::<Vec<_>>();
    let mut directory = root;
    let mut chain = Vec::with_capacity(parts.len());
    chain.push(DirectoryV9::of(&meta));
    for part in &parts[..parts.len() - 1] {
        check(binding, cancel, deadline)?;
        directory = fd(unsafe {
            libc::openat(
                directory.as_raw_fd(),
                cstring(part)?.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        let meta = directory.metadata()?;
        if !meta.is_dir() {
            return Err(unavailable());
        }
        chain.push(DirectoryV9::of(&meta));
    }
    Ok(ParentV9 {
        directory,
        name: cstring(parts[parts.len() - 1])?,
        chain,
    })
}
fn open_leaf(parent: &ParentV9) -> GitResult<File> {
    let mut raw = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.directory.as_raw_fd(),
            parent.name.as_ptr(),
            raw.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(unavailable());
    }
    // Reject observed symlink/special type BEFORE open, and recheck the actual
    // opened descriptor below in case an external writer races this boundary.
    if unsafe { raw.assume_init() }.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Err(unavailable());
    }
    let file = fd(unsafe {
        libc::openat(
            parent.directory.as_raw_fd(),
            parent.name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    })?;
    let meta = file.metadata()?;
    // Nonblocking avoids a FIFO wait before this actual descriptor type check;
    // only a regular single-link owned file can become an editable preimage.
    let uid = unsafe { libc::geteuid() };
    if !meta.is_file()
        || meta.uid() != uid
        || meta.nlink() != 1
        || meta.mode() & 0o6000 != 0
        || meta.len() > crate::scoped_actions_workflow_edit_v9::YAML_BYTES_V9 as u64
    {
        return Err(unavailable());
    }
    Ok(file)
}
impl WorkflowWorkingFileV9 {
    pub(super) fn mode_matches_saved_v9(&self, mode: &str) -> bool {
        matches!(mode, "100644" | "100755") && (self.leaf.mode & 0o111 != 0) == (mode == "100755")
    }
    #[cfg(test)]
    pub(crate) fn with_save_rename_observer_v9<T>(
        observe: impl FnOnce(Instant) + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        struct Clear;
        impl Drop for Clear {
            fn drop(&mut self) {
                BEFORE_SAVE_RENAME_V9.with(|slot| drop(slot.borrow_mut().take()));
            }
        }
        BEFORE_SAVE_RENAME_V9.with(|slot| {
            assert!(slot.borrow().is_none());
            *slot.borrow_mut() = Some(Box::new(observe));
        });
        let _clear = Clear;
        action()
    }
    pub(crate) fn save_confirmed_v9(
        self: &Arc<Self>,
        release: &crate::scoped_actions_workflow_edit_v9::WorkflowSaveReleaseV9<'_>,
    ) -> Result<Arc<Self>, bool> {
        let cancel = release.cancellation();
        let deadline = release.deadline();
        let prepared = (|| -> GitResult<ParentV9> {
            release.check(&self.binding).map_err(|_| unavailable())?;
            if !Arc::ptr_eq(self, release.preimage()) || release.content().len() > 256 * 1024 {
                return Err(unavailable());
            }
            self.validate(cancel, deadline)?;
            let target = parent(&self.binding, cancel, deadline)?;
            let uid = unsafe { libc::geteuid() };
            if target.chain != self.parents
                || target
                    .chain
                    .iter()
                    .any(|directory| directory.uid != uid || directory.mode & 0o022 != 0)
            {
                return Err(unavailable());
            }
            Ok(target)
        })()
        .map_err(|_| false)?;
        let mut temporary = None;
        for _ in 0..8 {
            check(&self.binding, cancel, deadline).map_err(|_| false)?;
            let serial = SAVE_SERIAL_V9
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| false)?;
            let name =
                cstring(format!(".ghtui-workflow-v9-{}-{serial}", std::process::id()).as_bytes())
                    .map_err(|_| false)?;
            let raw = unsafe {
                libc::openat(
                    prepared.directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_WRONLY
                        | libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                )
            };
            if raw >= 0 {
                temporary = Some(SaveTemporaryV9 {
                    parent: &prepared,
                    name,
                    file: fd(raw).map_err(|_| true)?,
                    renamed: false,
                });
                break;
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
                return Err(false);
            }
        }
        let mut temporary = temporary.ok_or(false)?;
        let changed = (|| -> GitResult<Arc<Self>> {
            for chunk in release.content().chunks(64 * 1024) {
                release.check(&self.binding).map_err(|_| unavailable())?;
                temporary.file.write_all(chunk)?;
            }
            release.check(&self.binding).map_err(|_| unavailable())?;
            temporary.file.sync_all()?;
            release.check(&self.binding).map_err(|_| unavailable())?;
            self.validate(cancel, deadline)?;
            let final_parent = parent(&self.binding, cancel, deadline)?;
            if final_parent.chain != prepared.chain {
                return Err(unavailable());
            }
            #[cfg(test)]
            BEFORE_SAVE_RENAME_V9.with(|slot| {
                let observe = slot.borrow_mut().take();
                if let Some(observe) = observe {
                    observe(deadline);
                }
            });
            release.check(&self.binding).map_err(|_| unavailable())?;
            // Atomic directory entry replacement, NOT an OS compare/exchange
            // against arbitrary external writers. The final descriptor/chain
            // check is adjacent; the UI must state the residual FS race.
            if unsafe {
                libc::renameat(
                    prepared.directory.as_raw_fd(),
                    temporary.name.as_ptr(),
                    final_parent.directory.as_raw_fd(),
                    final_parent.name.as_ptr(),
                )
            } != 0
            {
                return Err(unavailable());
            }
            temporary.renamed = true;
            // Keep private temporary mode0600 through rename; restore exact
            // ordinary original permission bits on the same actual FD only.
            release.check(&self.binding).map_err(|_| unavailable())?;
            temporary
                .file
                .set_permissions(std::fs::Permissions::from_mode(self.leaf.mode & 0o777))?;
            release.check(&self.binding).map_err(|_| unavailable())?;
            temporary.file.sync_all()?;
            release.check(&self.binding).map_err(|_| unavailable())?;
            final_parent.directory.sync_all()?;
            release.check(&self.binding).map_err(|_| unavailable())?;
            let saved = Self::read(self.binding.clone(), cancel, deadline)?;
            release.check(&self.binding).map_err(|_| unavailable())?;
            let meta = temporary.file.metadata()?;
            if saved.leaf.dev != meta.dev()
                || saved.leaf.ino != meta.ino()
                || saved.content() != release.content()
            {
                return Err(unavailable());
            }
            release.check(&self.binding).map_err(|_| unavailable())?;
            Ok(saved)
        })();
        // Any failure after a temporary file was created is inspection-only;
        // no claim that rollback/cleanup proved absence of a local effect.
        changed.map_err(|_| true)
    }
    pub(super) fn read(
        binding: Arc<WorkflowLocalBindingV9>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GitResult<Arc<Self>> {
        Self::read_observed(binding, cancel, deadline, || {})
    }
    pub(super) fn read_observed(
        binding: Arc<WorkflowLocalBindingV9>,
        cancel: &CancellationToken,
        deadline: Instant,
        after_actual_read: impl FnOnce(),
    ) -> GitResult<Arc<Self>> {
        let opened = parent(&binding, cancel, deadline)?;
        let mut file = open_leaf(&opened)?;
        let leaf = LeafV9::of(&file.metadata()?);
        let mut content = Vec::with_capacity(leaf.bytes as usize);
        let mut block = [0u8; 64 * 1024];
        loop {
            check(&binding, cancel, deadline)?;
            let count = file.read(&mut block)?;
            if count == 0 {
                break;
            }
            if content
                .len()
                .checked_add(count)
                .is_none_or(|n| n > leaf.bytes as usize)
            {
                return Err(unavailable());
            }
            content.extend_from_slice(&block[..count]);
        }
        if content.len() != leaf.bytes as usize || LeafV9::of(&file.metadata()?) != leaf {
            return Err(unavailable());
        }
        after_actual_read();
        check(&binding, cancel, deadline)?;
        // Rewalk from actual captured root, never only the old parent FD.
        let fresh = parent(&binding, cancel, deadline)?;
        let current = open_leaf(&fresh)?;
        if fresh.chain != opened.chain || LeafV9::of(&current.metadata()?) != leaf {
            return Err(unavailable());
        }
        let value = Arc::new(Self {
            binding,
            leaf,
            parents: opened.chain,
            content: content.into(),
        });
        check(&value.binding, cancel, deadline)?;
        Ok(value)
    }
    pub(crate) fn content(&self) -> &[u8] {
        &self.content
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 4 * std::mem::size_of::<usize>()
            + 2 * (std::mem::align_of::<usize>() - 1)
            + self.content.len()
            + self.parents.capacity() * std::mem::size_of::<DirectoryV9>()
    }
    pub(crate) fn validate(&self, cancel: &CancellationToken, deadline: Instant) -> GitResult<()> {
        let fresh = Self::read(self.binding.clone(), cancel, deadline)?;
        if fresh.leaf != self.leaf || fresh.parents != self.parents || fresh.content != self.content
        {
            return Err(unavailable());
        }
        check(&self.binding, cancel, deadline)
    }
    /// Closed cleanup-only descriptor/chain check. The immutable saved body
    /// already exists; no body clone/reparse or Git command follows last UID.
    pub(crate) fn validate_cleanup_descriptor_v9(
        &self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GitResult<()> {
        let fresh = parent(&self.binding, cancel, deadline)?;
        let file = open_leaf(&fresh)?;
        if fresh.chain != self.parents || LeafV9::of(&file.metadata()?) != self.leaf {
            return Err(unavailable());
        }
        check(&self.binding, cancel, deadline)
    }
}
