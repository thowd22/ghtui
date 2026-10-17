//! One serial owned service. At most the service, duplex tunnel and stderr
//! drainer are live I/O threads (3<=6), one TLS child and one CGI child.
use super::*;

pub(super) struct Service {
    pub openssl: PathBuf,
    pub backend: PathBuf,
    pub home: PathBuf,
    pub ca: PathBuf,
    pub key: PathBuf,
    pub gitroot: PathBuf,
}

struct Workers {
    stop: Arc<AtomicBool>,
    state: Arc<Shared>,
    tunnel: Option<JoinHandle<Result<(), String>>>,
    drain: Option<JoinHandle<Result<(), String>>>,
}
impl Workers {
    fn tunnel_finished(&self) -> bool {
        self.tunnel.as_ref().is_none_or(JoinHandle::is_finished)
    }
    fn finish(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Release);
        let tunnel = self.tunnel.take().map(|h| join(h, &self.state)).transpose();
        let drain = self.drain.take().map(|h| join(h, &self.state)).transpose();
        if tunnel.is_err() || drain.is_err() {
            self.state.failed.store(true, Ordering::Release);
            self.state.stop.store(true, Ordering::Release);
        }
        tunnel?;
        drain?;
        Ok(())
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

impl Service {
    pub(super) fn run(self, listener: TcpListener, state: &Arc<Shared>) -> Result<(), String> {
        loop {
            if state.stop.load(Ordering::Acquire) {
                return Ok(());
            }
            if Instant::now() >= state.deadline {
                return Err("TLS fixture exceeded30s service lifetime".into());
            }
            process::wait_fd(
                listener.as_raw_fd(),
                libc::POLLIN,
                &state.stop,
                state.deadline,
            )
            .or_else(|e| {
                if state.stop.load(Ordering::Acquire) {
                    Ok(())
                } else {
                    Err(e)
                }
            })?;
            if state.stop.load(Ordering::Acquire) {
                return Ok(());
            }
            let (client, peer) = match listener.accept() {
                Ok(pair) => pair,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(_) => return Err("Owned CONNECT listener accept failed".into()),
            };
            if !peer.ip().is_loopback() {
                return Err("Nonloopback fixture CONNECT refused".into());
            }
            if state.exchanges.fetch_add(1, Ordering::AcqRel) >= EXCHANGES {
                return Err("TLS fixture exceeds16 wire exchanges (not product READ roles)".into());
            }
            if let Err(error) = self.exchange(client, state) {
                if state.stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                state.stop.store(true, Ordering::Release);
                return Err(error);
            }
        }
    }
    fn exchange(&self, mut client: TcpStream, state: &Arc<Shared>) -> Result<(), String> {
        let deadline = (Instant::now() + Duration::from_secs(8)).min(state.deadline);
        client
            .set_nonblocking(true)
            .map_err(|_| "Fixture CONNECT nonblocking failed")?;
        let headers = protocol::read_headers(&mut client, &state.stop, deadline)?;
        protocol::connect(&headers)?;
        // Ephemeral handoff is NOT an ownership proof. The child must remain
        // an actual owned session, and only decrypted HTTP on its OWN stdout
        // can invoke CGI. Git itself verifies this private CA+SAN over TLS;
        // a racing foreign listener cannot supply that private key or broker.
        let reserve = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| "TLS port reservation failed")?;
        let address = reserve.local_addr().map_err(|_| "TLS port unavailable")?;
        drop(reserve);
        let mut command = Command::new(&self.openssl);
        command
            .args(["s_server", "-accept"])
            .arg(address.to_string())
            .arg("-cert")
            .arg(&self.ca)
            .arg("-key")
            .arg(&self.key)
            .args(["-quiet", "-no_ign_eof", "-naccept", "1"]);
        let mut tls = process::OwnedChild::spawn(command, &self.home, &state.processes)?;
        let mut encrypted = loop {
            process::alive(&state.stop, deadline)?;
            if !tls.running()? {
                return Err(
                    "Owned OpenSSL failed to bind/listen; no foreign-listener fallback".into(),
                );
            }
            match TcpStream::connect_timeout(&address, Duration::from_millis(20)) {
                Ok(socket) => break socket,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::ConnectionRefused
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    thread::yield_now()
                }
                Err(_) => return Err("Owned TLS loopback connect failed".into()),
            }
        };
        if !tls.running()? {
            return Err("Owned OpenSSL exited before exact CONNECT publication".into());
        }
        encrypted
            .set_nonblocking(true)
            .map_err(|_| "TLS socket flags failed")?;
        let fd = client.as_raw_fd();
        process::write_all(
            &mut client,
            fd,
            b"HTTP/1.1 200 Connection Established\r\n\r\n",
            &state.stop,
            deadline,
        )?;
        let (mut application_in, mut application_out) = tls.pipes()?;
        let mut stderr = tls.stderr()?;
        let local_stop = Arc::new(AtomicBool::new(false));
        let tunnel_stop = local_stop.clone();
        let tunnel_state = state.clone();
        let tunnel = thread::Builder::new()
            .name("w9-tls-tunnel".into())
            .spawn(move || {
                let result = tunnel(
                    &mut client,
                    &mut encrypted,
                    &tunnel_stop,
                    &tunnel_state.stop,
                    deadline,
                );
                if result.is_err() {
                    tunnel_state.failed.store(true, Ordering::Release);
                    tunnel_state.stop.store(true, Ordering::Release);
                }
                result
            })
            .map_err(|_| "TLS tunnel worker spawn failed")?;
        // Retain both handles under a cancellation/join Drop guard before any
        // further allocation/spawn/CGI work, including the unwind path.
        let mut workers = Workers {
            stop: local_stop.clone(),
            state: state.clone(),
            tunnel: Some(tunnel),
            drain: None,
        };
        let drain_stop = local_stop.clone();
        let drain_state = state.clone();
        let drain = thread::Builder::new()
            .name("w9-tls-stderr".into())
            .spawn(move || {
                let mut total = 0usize;
                loop {
                    if drain_stop.load(Ordering::Acquire)
                        || drain_state.stop.load(Ordering::Acquire)
                    {
                        return Ok(());
                    }
                    if let Err(error) =
                        process::wait_fd(stderr.as_raw_fd(), libc::POLLIN, &drain_stop, deadline)
                    {
                        if drain_stop.load(Ordering::Acquire)
                            || drain_state.stop.load(Ordering::Acquire)
                        {
                            return Ok(());
                        }
                        return Err(error);
                    }
                    let mut bytes = [0; 4096];
                    match stderr.read(&mut bytes) {
                        Ok(0) => return Ok(()),
                        Ok(n) => {
                            total = total.checked_add(n).ok_or("TLS stderr size overflow")?;
                            if total > 64 * 1024 {
                                drain_state.failed.store(true, Ordering::Release);
                                drain_state.stop.store(true, Ordering::Release);
                                return Err(
                                    "TLS stderr exceeds64KiB; private diagnostics withheld".into(),
                                );
                            }
                        }
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                            ) => {}
                        Err(_) => return Err("TLS stderr drain failed".into()),
                    }
                }
            })
            .map_err(|_| "TLS stderr worker spawn failed")?;
        workers.drain = Some(drain);
        let result: Result<(), String> = (|| {
            let request = protocol::read_request(&mut application_out, &state.stop, deadline)?;
            // The exact HTTP request came from this actual owned TLS child's
            // decrypted stdout, not a guessed listening port or fake response.
            if !tls.running()? {
                return Err("Actual owned TLS session vanished before backend".into());
            }
            let mut backend = Command::new(&self.backend);
            backend
                .env("GIT_PROJECT_ROOT", &self.gitroot)
                .env("GIT_HTTP_EXPORT_ALL", "1")
                .env("PATH_INFO", request.path_info)
                .env("QUERY_STRING", request.query)
                .env("REQUEST_METHOD", request.method)
                .env("CONTENT_TYPE", request.content_type)
                .env("CONTENT_LENGTH", request.body.len().to_string())
                .env("SERVER_PROTOCOL", "HTTP/1.1")
                .env("SERVER_NAME", "ghe.example")
                .env("REMOTE_ADDR", "127.0.0.1");
            if request.protocol_v2 {
                backend.env("HTTP_GIT_PROTOCOL", "version=2");
            }
            let output = process::OwnedChild::spawn(backend, &self.home, &state.processes)?
                .collect(&request.body, protocol::STREAM_CAP, &state.stop, deadline)?;
            let (status, response) = protocol::response(&output)?;
            let fd = application_in.as_raw_fd();
            process::write_all(&mut application_in, fd, &response, &state.stop, deadline)?;
            state
                .requests
                .lock()
                .map_err(|_| "TLS request metadata poisoned")?
                .push(RequestMetadata {
                    method: request.method,
                    path: request.path,
                    status,
                    request_bytes: request.bytes,
                    response_bytes: response.len(),
                });
            Ok(())
        })();
        // EOF tells -no_ign_eof to flush encrypted output/close_notify. Keep
        // the tunnel running until actual EOF, not a guessed paint/delay.
        drop(application_in);
        drop(application_out);
        if result.is_ok() {
            while !workers.tunnel_finished()
                && !state.stop.load(Ordering::Acquire)
                && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(1));
            }
        }
        local_stop.store(true, Ordering::Release);
        let cleaned = tls.cleanup();
        let workers_result = workers.finish();
        result?;
        cleaned?;
        workers_result?;
        Ok(())
    }
}

fn join<T>(handle: JoinHandle<Result<T, String>>, state: &Shared) -> Result<T, String> {
    let until = Instant::now() + Duration::from_secs(1);
    while !handle.is_finished() && Instant::now() < until {
        thread::sleep(Duration::from_millis(1));
    }
    if !handle.is_finished() {
        return Err("Owned TLS I/O worker did not finish in its bounded shutdown".into());
    }
    state.joined.fetch_add(1, Ordering::AcqRel);
    handle.join().map_err(|_| "Owned TLS I/O worker panicked")?
}

fn tunnel(
    client: &mut TcpStream,
    tls: &mut TcpStream,
    local: &AtomicBool,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<(), String> {
    let (mut client_read, mut tls_read) = (true, true);
    let (mut up, mut down) = (Vec::<u8>::new(), Vec::<u8>::new());
    let (mut up_at, mut down_at, mut up_count, mut down_count) = (0, 0, 0usize, 0usize);
    let mut bytes = [0; 8192];
    loop {
        if local.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
            break;
        }
        if let Err(error) = process::alive(stop, deadline) {
            if local.load(Ordering::Acquire) || stop.load(Ordering::Acquire) {
                break;
            }
            return Err(error);
        }
        for (reader, open, pending, at, count) in [
            (
                &mut *client,
                &mut client_read,
                &mut up,
                &mut up_at,
                &mut up_count,
            ),
            (
                &mut *tls,
                &mut tls_read,
                &mut down,
                &mut down_at,
                &mut down_count,
            ),
        ] {
            if !*open || pending.len() != *at {
                continue;
            }
            pending.clear();
            *at = 0;
            match reader.read(&mut bytes) {
                Ok(0) => *open = false,
                Ok(n) => {
                    *count = count
                        .checked_add(n)
                        .ok_or("Encrypted TLS stream size overflow")?;
                    if *count > protocol::STREAM_CAP {
                        return Err("Encrypted TLS stream exceeds1MiB including framing".into());
                    }
                    pending.extend_from_slice(&bytes[..n]);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err("Owned encrypted TLS tunnel read failed".into()),
            }
        }
        for (writer, pending, at) in [
            (&mut *tls, &up, &mut up_at),
            (&mut *client, &down, &mut down_at),
        ] {
            if *at == pending.len() {
                continue;
            }
            match writer.write(&pending[*at..]) {
                Ok(0) => return Err("Owned encrypted TLS tunnel short write".into()),
                Ok(n) => *at += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => return Err("Owned encrypted TLS tunnel write failed".into()),
            }
        }
        if !client_read && up_at == up.len() {
            let _ = tls.shutdown(Shutdown::Write);
        }
        if !tls_read && down_at == down.len() {
            let _ = client.shutdown(Shutdown::Write);
            break;
        }
        let mut polls = [
            libc::pollfd {
                fd: client.as_raw_fd(),
                events: libc::POLLIN
                    | if down_at < down.len() {
                        libc::POLLOUT
                    } else {
                        0
                    },
                revents: 0,
            },
            libc::pollfd {
                fd: tls.as_raw_fd(),
                events: libc::POLLIN | if up_at < up.len() { libc::POLLOUT } else { 0 },
                revents: 0,
            },
        ];
        // SAFETY: initialized owned socket descriptors; finite2ms poll.
        unsafe {
            libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, 2);
        }
    }
    let _ = client.shutdown(Shutdown::Both);
    let _ = tls.shutdown(Shutdown::Both);
    Ok(())
}
