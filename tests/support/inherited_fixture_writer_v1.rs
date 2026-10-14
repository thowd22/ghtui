//! Linux deterministic fork window for fixture publication tests only. The
//! owned child holds ONLY explicit test files and its two synchronization FDs.
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::{Path, PathBuf},
};

pub struct HeldFixtureWriter {
    pid: i32,
    release: Option<File>,
    paths: Vec<PathBuf>,
}
fn pipe() -> io::Result<(File, File)> {
    let mut pair = [0; 2];
    // SAFETY: valid two-int output; each descriptor is immediately owned.
    if unsafe { libc::pipe2(pair.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { (File::from_raw_fd(pair[0]), File::from_raw_fd(pair[1])) })
}
impl HeldFixtureWriter {
    pub fn hold(paths: &[&Path]) -> io::Result<Self> {
        if paths.is_empty() || paths.len() > 8 {
            return Err(io::Error::other(
                "One to eight exact owned fixture files required",
            ));
        }
        let writers = paths
            .iter()
            .map(|p| fs::OpenOptions::new().write(true).open(p))
            .collect::<io::Result<Vec<_>>>()?;
        let (mut ready_read, ready_write) = pipe()?;
        let (release_read, release_write) = pipe()?;
        let mut keep = Vec::new();
        for fd in writers
            .iter()
            .map(AsRawFd::as_raw_fd)
            .chain([ready_write.as_raw_fd(), release_read.as_raw_fd()])
        {
            // Originals >=128 cannot be clobbered by the child's low-FD copy.
            let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 128) };
            if duplicate < 0 {
                return Err(io::Error::last_os_error());
            }
            keep.push(unsafe { OwnedFd::from_raw_fd(duplicate) });
        }
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            // No allocation, locks, formatting or destructor runs after fork.
            // Close unrelated inherited FDs before holding the intentional
            // window; otherwise this regression could poison other fixtures.
            unsafe {
                for (i, fd) in keep.iter().enumerate() {
                    if libc::dup2(fd.as_raw_fd(), 3 + i as i32) < 0 {
                        libc::_exit(91)
                    }
                }
                if libc::close_range(3 + keep.len() as u32, u32::MAX, 0) != 0 {
                    libc::_exit(92)
                }
                let ready = 3 + paths.len() as i32;
                let release = ready + 1;
                if libc::write(ready, b"r".as_ptr().cast(), 1) != 1 {
                    libc::_exit(93)
                }
                let mut byte = 0u8;
                loop {
                    let value = libc::read(release, (&mut byte as *mut u8).cast(), 1);
                    if value >= 0 {
                        libc::_exit(0)
                    }
                    if *libc::__errno_location() != libc::EINTR {
                        libc::_exit(94)
                    }
                }
            }
        }
        drop(keep);
        drop(writers);
        drop(ready_write);
        drop(release_read);
        let mut result = Self {
            pid,
            release: Some(release_write),
            paths: paths.iter().map(|p| p.to_path_buf()).collect(),
        };
        let mut byte = [0];
        if let Err(error) = ready_read.read_exact(&mut byte) {
            result.finish()?;
            return Err(error);
        }
        if byte != *b"r" {
            return Err(io::Error::other("Wrong actual child acknowledgment"));
        }
        // Independent kernel evidence: exact child FDs still refer to the
        // explicit fixture files after ALL publishing-parent writers closed.
        for (index, path) in result.paths.iter().enumerate() {
            if fs::read_link(format!("/proc/{pid}/fd/{}", index + 3))? != *path {
                return Err(io::Error::other("Owned inherited writer target differs"));
            }
        }
        Ok(result)
    }
    pub fn pid(&self) -> i32 {
        self.pid
    }
    pub fn finish(&mut self) -> io::Result<()> {
        let Some(mut release) = self.release.take() else {
            return Ok(());
        };
        let release_result = release.write_all(b"x");
        drop(release);
        let mut status = 0;
        loop {
            let result = unsafe { libc::waitpid(self.pid, &mut status, 0) };
            if result == self.pid {
                break;
            }
            if result < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(io::Error::last_os_error());
        }
        if status != 0 {
            return Err(io::Error::other(format!(
                "Owned writer child failed {status}"
            )));
        }
        release_result?;
        Ok(())
    }
}
impl Drop for HeldFixtureWriter {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            if std::thread::panicking() {
                eprintln!("Owned fixture writer cleanup failed: {error}");
            } else {
                panic!("Owned fixture writer cleanup failed: {error}");
            }
        }
    }
}
