//! Preserve Tantivy's mmap storage and locking protocol, but explicitly unlock before close.
//! A concurrently forked, not-yet-exec'ed Git/gh child must not prolong a finished writer's lock.
use super::SourceLock;
use std::{
    fs::OpenOptions,
    io,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tantivy::directory::{
    error::{DeleteError, LockError, OpenDirectoryError, OpenReadError, OpenWriteError},
    Directory, DirectoryLock, FileHandle, Lock, MmapDirectory, WatchCallback, WatchHandle,
    WritePtr,
};

#[derive(Clone, Debug)]
pub(super) struct IndexDirectory {
    mmap: MmapDirectory,
    root: PathBuf,
}

impl IndexDirectory {
    pub(super) fn open(root: &Path) -> Result<Self, OpenDirectoryError> {
        Ok(Self {
            mmap: MmapDirectory::open(root)?,
            root: root.to_owned(),
        })
    }
}

impl Directory for IndexDirectory {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        self.mmap.get_file_handle(path)
    }

    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        self.mmap.delete(path)
    }

    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        self.mmap.exists(path)
    }

    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        self.mmap.open_write(path)
    }

    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        self.mmap.atomic_read(path)
    }

    fn atomic_write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.mmap.atomic_write(path, data)
    }

    fn sync_directory(&self) -> io::Result<()> {
        self.mmap.sync_directory()
    }

    fn watch(&self, callback: WatchCallback) -> tantivy::Result<WatchHandle> {
        self.mmap.watch(callback)
    }

    fn acquire_lock(&self, lock: &Lock) -> Result<DirectoryLock, LockError> {
        use std::os::fd::AsRawFd;
        // Tantivy's writer/meta names are single directory members. Do not let an unexpected
        // caller turn this narrower adapter into an arbitrary lock-file opener.
        let mut components = lock.filepath.components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return Err(LockError::wrap_io_error(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Index lock must be one direct directory member",
            )));
        }
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(self.root.join(&lock.filepath))
            .map_err(LockError::wrap_io_error)?;
        let metadata = file.metadata().map_err(LockError::wrap_io_error)?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(LockError::wrap_io_error(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Index lock must be an owned regular unshared file",
            )));
        }
        let operation = libc::LOCK_EX | if lock.is_blocking { 0 } else { libc::LOCK_NB };
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), operation) } == 0 {
                return Ok(DirectoryLock::from(Box::new(SourceLock(file))));
            }
            let error = io::Error::last_os_error();
            if lock.is_blocking && error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(if error.kind() == io::ErrorKind::WouldBlock {
                LockError::LockBusy
            } else {
                LockError::wrap_io_error(error)
            });
        }
    }
}
