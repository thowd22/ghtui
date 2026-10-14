// A real pre-exec child holds the inherited descriptors. No Rust runtime work occurs in the
// child after a multithreaded fork, and the parent always kills and reaps it on assertion failure.
struct IndexPreExecChild {
    pid: libc::pid_t,
    _release: File,
}

impl IndexPreExecChild {
    fn hold_inherited_descriptors() -> Self {
        use std::os::fd::FromRawFd;
        let mut ready = [0; 2];
        let mut release = [0; 2];
        assert_eq!(unsafe { libc::pipe(ready.as_mut_ptr()) }, 0);
        let ready_read = unsafe { File::from_raw_fd(ready[0]) };
        let ready_write = unsafe { File::from_raw_fd(ready[1]) };
        assert_eq!(unsafe { libc::pipe(release.as_mut_ptr()) }, 0);
        let release_read = unsafe { File::from_raw_fd(release[0]) };
        let release_write = unsafe { File::from_raw_fd(release[1]) };
        let pid = unsafe { libc::fork() };
        assert!(pid >= 0);
        if pid == 0 {
            unsafe {
                libc::close(ready[0]);
                libc::close(release[1]);
                let byte = b'x';
                if libc::write(ready[1], (&byte as *const u8).cast(), 1) != 1 {
                    libc::_exit(91);
                }
                let mut byte = 0u8;
                libc::read(release[0], (&mut byte as *mut u8).cast(), 1);
                libc::_exit(0);
            }
        }
        let child = Self {
            pid,
            _release: release_write,
        };
        drop(ready_write);
        drop(release_read);
        let mut poll = libc::pollfd {
            fd: ready_read.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut poll, 1, 2000) }, 1);
        let mut byte = 0u8;
        assert_eq!(
            unsafe { libc::read(ready_read.as_raw_fd(), (&mut byte as *mut u8).cast(), 1) },
            1
        );
        assert_eq!(byte, b'x');
        child.assert_alive();
        child
    }

    fn assert_alive(&self) {
        assert_eq!(unsafe { libc::kill(self.pid, 0) }, 0);
        assert_eq!(
            unsafe { libc::waitpid(self.pid, std::ptr::null_mut(), libc::WNOHANG) },
            0,
            "The held pre-exec child must not be an already-exited zombie"
        );
    }
}

impl Drop for IndexPreExecChild {
    fn drop(&mut self) {
        // Keep the release pipe open until the deliberately held child is killed/reaped.
        unsafe {
            libc::kill(self.pid, libc::SIGKILL);
            while libc::waitpid(self.pid, std::ptr::null_mut(), 0) < 0
                && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted
            {}
        }
    }
}

#[test]
fn search_index_writer_drop_releases_actual_tantivy_lock_before_inherited_child_exec() {
    let directory = tempfile::tempdir().unwrap();
    let mut cache = Cache::memory().unwrap();
    let cancel = CancellationToken::default();
    let mut index = RankedIndex::open(
        directory.path(),
        scope(),
        &cache,
        RankedLimits::default(),
        &cancel,
    )
    .unwrap();
    let writer: IndexWriter = index.index.writer_with_num_threads(1, 50_000_000).unwrap();
    let child = IndexPreExecChild::hold_inherited_descriptors();
    writer.wait_merging_threads().unwrap();
    child.assert_alive();
    let first = index
        .publish(
            &mut cache,
            "chosen",
            1,
            &corpus(vec![file("first.rs", "firstgenerationneedle")], true),
            &cancel,
        )
        .expect("The writer was joined and dropped; a pre-exec child is not another index writer");
    assert_eq!(first.changed, 1);
    assert_eq!(search(&index, "firstgenerationneedle").hits.len(), 1);
    let second = index
        .publish(
            &mut cache,
            "chosen",
            2,
            &corpus(vec![file("second.rs", "secondgenerationneedle")], true),
            &cancel,
        )
        .unwrap();
    assert!(second.index_generation > first.index_generation);
    assert_eq!((second.changed, second.deleted), (1, 1));
    assert!(search(&index, "firstgenerationneedle").hits.is_empty());
    assert_eq!(search(&index, "secondgenerationneedle").hits.len(), 1);
    child.assert_alive();
    drop(child);
}

#[test]
fn search_index_real_tantivy_writer_contention_is_still_nonblocking_and_exclusive() {
    let directory = tempfile::tempdir().unwrap();
    let cache = Cache::memory().unwrap();
    let cancel = CancellationToken::default();
    let index = RankedIndex::open(
        directory.path(),
        scope(),
        &cache,
        RankedLimits::default(),
        &cancel,
    )
    .unwrap();
    let writer: IndexWriter = index.index.writer_with_num_threads(1, 50_000_000).unwrap();
    assert!(matches!(
        index.index.writer_with_num_threads::<TantivyDocument>(1, 50_000_000),
        Err(tantivy::TantivyError::LockFailure(
            tantivy::directory::error::LockError::LockBusy,
            _
        ))
    ));
    writer.wait_merging_threads().unwrap();
    let next: IndexWriter = index.index.writer_with_num_threads(1, 50_000_000).unwrap();
    next.wait_merging_threads().unwrap();
    assert!(cache.list(&scope(), None).unwrap().is_empty());
    assert!(cache.sync_checkpoint(&scope(), &checkpoint("chosen")).unwrap().is_none());
}

#[test]
fn search_index_lock_adapter_refuses_symlinks_hardlinks_and_nonmember_paths_without_effects() {
    use std::os::unix::fs::symlink;
    use tantivy::directory::{Directory, Lock};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("index");
    fs::create_dir(&root).unwrap();
    let adapter = index_directory::IndexDirectory::open(&root).unwrap();
    let outside = directory.path().join("outside");
    fs::write(&outside, b"must stay unchanged").unwrap();
    let before = fs::read(&outside).unwrap();
    for path in ["../outside", "nested/lock", "/absolute-lock", "."] {
        assert!(adapter.acquire_lock(&Lock {
            filepath: path.into(),
            is_blocking: false,
        }).is_err());
    }
    symlink(&outside, root.join("symlink.lock")).unwrap();
    fs::hard_link(&outside, root.join("hardlink.lock")).unwrap();
    fs::create_dir(root.join("directory.lock")).unwrap();
    for path in ["symlink.lock", "hardlink.lock", "directory.lock"] {
        assert!(adapter.acquire_lock(&Lock {
            filepath: path.into(),
            is_blocking: false,
        }).is_err());
    }
    let valid = Lock {
        filepath: "valid.lock".into(),
        is_blocking: false,
    };
    let guard = adapter.acquire_lock(&valid).unwrap();
    assert!(matches!(adapter.acquire_lock(&valid), Err(tantivy::directory::error::LockError::LockBusy)));
    assert_eq!(fs::metadata(root.join("valid.lock")).unwrap().permissions().mode() & 0o777, 0o600);
    drop(guard);
    drop(adapter.acquire_lock(&valid).unwrap());
    assert_eq!(fs::read(&outside).unwrap(), before);
    assert!(!root.join("nested").exists());
}
