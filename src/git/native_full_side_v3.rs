//! Read-only full-side evidence. Raw working bytes are not a Git blob, patch
//! coordinate, or permission to mutate. No symlink target is ever followed.
use super::*;
use crate::{
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_view::{file_content_kind_v3, BlobPreviewKind, DiffSide},
};
use std::{
    ffi::CString,
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::fs::MetadataExt,
};

const SIDE_CAP: usize = 2 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    mode: u32,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
    parents: Vec<(u64, u64)>,
}
impl Stamp {
    fn of(meta: &std::fs::Metadata) -> Self {
        Self {
            dev: meta.dev(),
            ino: meta.ino(),
            mode: meta.mode(),
            size: meta.len(),
            modified: (meta.mtime(), meta.mtime_nsec()),
            changed: (meta.ctime(), meta.ctime_nsec()),
            parents: vec![],
        }
    }
}

#[derive(Debug)]
pub(crate) struct MutableFullSideV3 {
    source: Arc<MutableDiffSourceV2>,
    file: usize,
    side: DiffSide,
    content: Vec<u8>,
    kind: BlobPreviewKind,
    label: String,
    object: Option<String>,
    stamp: Option<Stamp>,
    absent: bool,
}
impl MutableFullSideV3 {
    pub(crate) fn read(
        source: Arc<MutableDiffSourceV2>,
        file: usize,
        side: DiffSide,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        if side == DiffSide::Both {
            return Err("Choose explicit Old or New full-file content".into());
        }
        let member = source
            .members()
            .get(file)
            .ok_or("Full-side member is outside genuine observed catalogue")?;
        source.validate_current(operation)?;
        let result=source.intent().origin().with_saved_navigation_read_v1(operation,|repo|{
            refuse_overrides(repo)?;
            let mut value=Self{source:source.clone(),file,side,content:vec![],kind:BlobPreviewKind::Text,label:String::new(),object:None,stamp:None,absent:false};
            let Some(path)=member.path(side) else {
                value.absent=true;
                value.label=format!("{side:?} side absent in genuine observed comparison • no substituted HEAD/index/worktree content");
                operation.check()?;
                return Ok(value);
            };
            let index=matches!((source.intent().kind(),side),(MutableDiffKindV2::Working,DiffSide::Old)|(MutableDiffKindV2::Staged,DiffSide::New));
            let worktree=side==DiffSide::New && source.intent().kind()!=MutableDiffKindV2::Staged;
            if worktree {
                let root=source.intent().origin().source();
                let (content,stamp,mode)=read_worktree(root,path,operation)?;
                // A directory is display-only gitlink metadata, never nested Git
                // discovery or the claim that its checked-out HEAD equals index.
                if mode=="160000" {
                    let entry=index_entry(repo,path)?;
                    if entry.mode!="160000" {return Err("Working directory is not the genuine tracked gitlink".into());}
                    value.object=Some(entry.id);
                }
                value.kind=file_content_kind_v3(&mode,mode=="160000",&content);
                value.label="RAW UNSAVED working-tree bytes • not Git-normalized patch lines, a saved blob, author identity or staging coordinates".into();
                if value.kind==BlobPreviewKind::Submodule{value.label="Observed WORKING gitlink directory • index pointer is NOT the nested checked-out HEAD; nested version/bytes were NOT read".into();}
                value.content=content;value.stamp=Some(stamp);
            } else {
                let entry=if index {index_entry(repo,path)?} else {
                    let head=source.result().base.as_deref().ok_or("Observed baseline has no saved HEAD; absent side must remain absent")?;
                    repo.tree_entry(head,path).map_err(|error|error.to_string())?
                };
                if !matches!((entry.mode.as_str(),entry.kind.as_str()),("100644"|"100755"|"120000","blob")|("160000","commit")) {return Err("Full-side object type/mode is unsupported".into());}
                if entry.mode!="160000" {
                    let size=repo.object_sizes(std::slice::from_ref(&entry.id)).map_err(|error|error.to_string())?.into_iter().next().ok_or("Full-side object size unavailable")?;
                    if size>SIDE_CAP {return Err("Full-file object exceeds2MiB; no body read, external renderer or fetch".into());}
                    value.content=repo.original_side_blob_capture_v1(&entry.id).map_err(|error|error.to_string())?;
                    if value.content.len()!=size {return Err("Full-side object size changed".into());}
                }
                value.kind=file_content_kind_v3(&entry.mode,entry.mode=="160000",&value.content);
                value.label=if index {format!("Observed INDEX stage0 object {} • not a saved commit or worktree bytes",entry.id)} else {format!("Observed saved HEAD {} • actual object {} • no requested-ref substitution",source.result().base.as_deref().unwrap_or("unavailable"),entry.id)};
                value.object=Some(entry.id);
            }
            refuse_overrides(repo)?;
            operation.check()?;Ok(value)
        })?;
        source.validate_current(operation)?;
        let result = Arc::new(result);
        operation.check()?;
        Ok(result)
    }
    /// Every later projection performs fresh same-source checks and a bounded
    /// exact byte comparison, not merely a metadata/timestamp comparison.
    pub(crate) fn validate_current(
        &self,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<(), String> {
        let fresh = Self::read(self.source.clone(), self.file, self.side, operation)?;
        if fresh.content != self.content
            || fresh.stamp != self.stamp
            || fresh.object != self.object
            || fresh.kind != self.kind
            || fresh.absent != self.absent
        {
            return Err(
                "Full-side content/evidence changed; return to patch and reload the comparison"
                    .into(),
            );
        }
        operation.check()
    }
    pub(crate) fn content(&self) -> &[u8] {
        &self.content
    }
    pub(crate) fn kind(&self) -> &BlobPreviewKind {
        &self.kind
    }
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
    pub(crate) fn object(&self) -> Option<&str> {
        self.object.as_deref()
    }
    pub(crate) fn absent(&self) -> bool {
        self.absent
    }
    pub(crate) fn raw_worktree(&self) -> bool {
        self.stamp.is_some()
    }
    pub(crate) fn observed_head(&self) -> bool {
        self.side == DiffSide::Old && self.source.intent().kind() != MutableDiffKindV2::Working
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.content.capacity()
            + self.label.capacity()
            + self.object.as_ref().map_or(0, String::capacity)
            + self.stamp.as_ref().map_or(0, |stamp| {
                stamp.parents.capacity() * std::mem::size_of::<(u64, u64)>()
            })
            + match &self.kind {
                BlobPreviewKind::LfsPointer { object, .. } => object.capacity(),
                _ => 0,
            }
            + 128
    }
}

fn refuse_overrides(repo: &GitRepository) -> Result<(), String> {
    let (replacements, grafts) = repo
        .history_overrides()
        .map_err(|error| error.to_string())?;
    if replacements != 0 || grafts {
        return Err("Full mutable side unavailable under replacement/graft history overrides; patch remains as Git interprets it. No original-content substitution was made.".into());
    }
    Ok(())
}

/// One bounded installed-Git NUL stage record, not another diff parser.
fn index_entry(repo: &GitRepository, path: &RawPath) -> Result<TreeEntry, String> {
    validate_path(path).map_err(|error| error.to_string())?;
    let output = repo
        .bytes_with_capture_limit_v2(
            [
                OsStr::new("ls-files"),
                OsStr::new("--stage"),
                OsStr::new("-z"),
                OsStr::new("--"),
                path.as_os_str(),
            ],
            None,
            16 * 1024,
        )
        .map_err(|error| error.to_string())?;
    let record = output
        .strip_suffix(&[0])
        .ok_or("Index side requires a complete NUL record")?;
    if record.contains(&0) {
        return Err("Index side is ambiguous/unmerged; no stage0 fallback".into());
    }
    let tab = record
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or("Index side stage record has no path boundary")?;
    if record[tab + 1..] != path.0 {
        return Err("Index side returned a different original raw path".into());
    }
    let fields = record[..tab]
        .split(|byte| *byte == b' ')
        .collect::<Vec<_>>();
    if fields.len() != 3 || fields[2] != b"0" {
        return Err("Index side is not one genuine stage0 entry".into());
    }
    let mode = std::str::from_utf8(fields[0]).map_err(|_| "Invalid index mode")?;
    let id = std::str::from_utf8(fields[1]).map_err(|_| "Invalid index object")?;
    if !matches!(mode, "100644" | "100755" | "120000" | "160000") || !original_actor_saved_sha(id) {
        return Err("Index side mode/object is invalid".into());
    }
    Ok(TreeEntry {
        mode: mode.into(),
        kind: if mode == "160000" { "commit" } else { "blob" }.into(),
        id: id.into(),
        path: path.clone(),
    })
}

fn cstring(bytes: &[u8]) -> Result<CString, String> {
    CString::new(bytes).map_err(|_| "Raw full-side path contains NUL".into())
}
fn opened(fd: i32) -> Result<File, String> {
    if fd < 0 {
        return Err(format!(
            "No-follow full-side open refused: {}",
            io::Error::last_os_error()
        ));
    }
    // SAFETY: only a fresh owned successful open/openat descriptor is adopted.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn verify_root(root: &File, expected: (u64, u64)) -> Result<(), String> {
    let metadata = root.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_dir() || (metadata.dev(), metadata.ino()) != expected {
        return Err(
            "Opened root descriptor is not the captured original repository; no body read".into(),
        );
    }
    Ok(())
}
fn read_worktree(
    source: &LocalCommitSourceIdentity,
    path: &RawPath,
    operation: &LocalGitNavigationReadV1,
) -> Result<(Vec<u8>, Stamp, String), String> {
    read_worktree_with_recheck_v3(source, path, operation, || {})
}
fn read_worktree_with_recheck_v3(
    source: &LocalCommitSourceIdentity,
    path: &RawPath,
    operation: &LocalGitNavigationReadV1,
    after_actual_read: impl FnOnce(),
) -> Result<(Vec<u8>, Stamp, String), String> {
    let OpenedParentV3 {
        directory,
        name,
        chain,
    } = open_parent_v3(source, path, operation)?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(format!(
            "Full-side current member unavailable: {}",
            io::Error::last_os_error()
        ));
    }
    let mode = native_mode_v3(unsafe { stat.assume_init() }.st_mode) & native_mode_v3(libc::S_IFMT);
    if ![
        native_mode_v3(libc::S_IFREG),
        native_mode_v3(libc::S_IFLNK),
        native_mode_v3(libc::S_IFDIR),
    ]
    .contains(&mode)
    {
        return Err(
            "Full-side special file refused before open/body read; no device/FIFO/socket read"
                .into(),
        );
    }
    let mut file = opened(unsafe {
        libc::openat(directory.as_raw_fd(), name.as_ptr(), descriptor_flags(mode))
    })?;
    let before = file.metadata().map_err(|error| error.to_string())?;
    let mut stamp = Stamp::of(&before);
    if before.mode() & native_mode_v3(libc::S_IFMT) != mode {
        return Err("Full-side file type changed before read".into());
    }
    let (content, git_mode) = if before.is_file() {
        if before.len() > SIDE_CAP as u64 {
            return Err("RAW unsaved full file exceeds2MiB; body not read".into());
        }
        let mut content = Vec::with_capacity(before.len() as usize);
        let mut block = [0u8; 64 * 1024];
        loop {
            operation.check()?;
            let count = file.read(&mut block).map_err(|error| error.to_string())?;
            if count == 0 {
                break;
            }
            if content.len().saturating_add(count) > SIDE_CAP {
                return Err("RAW unsaved full file grew beyond2MiB".into());
            }
            content.extend_from_slice(&block[..count]);
        }
        if content.len() != before.len() as usize {
            return Err("RAW unsaved full file length contradicts captured descriptor size".into());
        }
        (
            content,
            if before.mode() & 0o111 != 0 {
                "100755"
            } else {
                "100644"
            },
        )
    } else if before.file_type().is_symlink() {
        (read_link_descriptor(&file)?, "120000")
    } else if before.is_dir() {
        (vec![], "160000")
    } else {
        return Err(
            "Full-side special file refused before body read; no device/FIFO/socket read".into(),
        );
    };
    after_actual_read();
    operation.check()?;
    if Stamp::of(&file.metadata().map_err(|error| error.to_string())?) != stamp {
        return Err("Full-side descriptor changed during bounded read".into());
    }
    // Rewalk FROM THE ACTUAL ORIGINAL ROOT, not merely the retained parent FD.
    // A renamed/detached old directory can otherwise preserve its old leaf even
    // while the current raw path points at an identical-content replacement.
    let fresh = open_parent_v3(source, path, operation)?;
    if fresh.chain != chain {
        return Err("Full-side original root-relative parent chain changed during read".into());
    }
    let current = opened(unsafe {
        libc::openat(
            fresh.directory.as_raw_fd(),
            fresh.name.as_ptr(),
            descriptor_flags(mode),
        )
    })?;
    if Stamp::of(&current.metadata().map_err(|error| error.to_string())?) != stamp {
        return Err("Full-side original name no longer identifies the observed descriptor".into());
    }
    stamp.parents = chain;
    operation.check()?;
    Ok((content, stamp, git_mode.into()))
}
struct OpenedParentV3 {
    directory: File,
    name: CString,
    chain: Vec<(u64, u64)>,
}
fn open_parent_v3(
    source: &LocalCommitSourceIdentity,
    path: &RawPath,
    operation: &LocalGitNavigationReadV1,
) -> Result<OpenedParentV3, String> {
    operation.check()?;
    validate_path(path).map_err(|error| error.to_string())?;
    if path.0.len() > 4096
        || path
            .0
            .split(|byte| *byte == b'/')
            .any(|part| part.is_empty() || part == b"." || part == b"..")
    {
        return Err("Full-side raw path is not a bounded relative member".into());
    }
    let name = cstring(source.root().as_os_str().as_bytes())?;
    let root = opened(unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    })?;
    verify_root(&root, source.root_file_identity_v3())?;
    let mut directory = root;
    let mut chain = vec![source.root_file_identity_v3()];
    let parts = path.0.split(|byte| *byte == b'/').collect::<Vec<_>>();
    for part in &parts[..parts.len() - 1] {
        operation.check()?;
        let part = cstring(part)?;
        directory = opened(unsafe {
            libc::openat(
                directory.as_raw_fd(),
                part.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        let metadata = directory.metadata().map_err(|error| error.to_string())?;
        chain.push((metadata.dev(), metadata.ino()));
    }
    let name = cstring(parts[parts.len() - 1])?;
    Ok(OpenedParentV3 {
        directory,
        name,
        chain,
    })
}
fn descriptor_flags(mode: u32) -> i32 {
    let ordinary =
        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_NOCTTY;
    if mode == native_mode_v3(libc::S_IFLNK) {
        #[cfg(target_os = "linux")]
        {
            return libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        }
        #[cfg(target_os = "macos")]
        {
            return libc::O_SYMLINK | libc::O_CLOEXEC;
        }
    }
    ordinary
}
fn native_mode_v3(mode: libc::mode_t) -> u32 {
    #[cfg(target_os = "linux")]
    {
        mode
    }
    #[cfg(target_os = "macos")]
    {
        u32::from(mode)
    }
}
fn read_link_descriptor(file: &File) -> Result<Vec<u8>, String> {
    let mut content = vec![0u8; 4097];
    #[cfg(target_os = "linux")]
    let count = unsafe {
        libc::readlinkat(
            file.as_raw_fd(),
            c"".as_ptr(),
            content.as_mut_ptr().cast(),
            content.len(),
        )
    };
    #[cfg(target_os = "macos")]
    let count = {
        // Apple exposes descriptor-bound freadlink since macOS13. Resolve it at
        // runtime so older supported binaries fail closed for this special view
        // instead of requiring a newer loader or racing a pathname fallback.
        let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"freadlink".as_ptr()) };
        if symbol.is_null() {
            return Err("RAW symlink literal needs descriptor freadlink (macOS13+); target was not followed".into());
        }
        let read: unsafe extern "C" fn(i32, *mut libc::c_char, usize) -> libc::ssize_t =
            unsafe { std::mem::transmute(symbol) };
        unsafe { read(file.as_raw_fd(), content.as_mut_ptr().cast(), content.len()) }
    };
    if count < 0 {
        return Err(format!(
            "Descriptor symlink literal unavailable: {}",
            io::Error::last_os_error()
        ));
    }
    if count as usize >= content.len() {
        return Err("RAW symlink literal exceeds4096bytes; target not followed".into());
    }
    content.truncate(count as usize);
    Ok(content)
}
#[cfg(test)]
#[path = "native_full_side_v3_tests.rs"]
mod tests;
