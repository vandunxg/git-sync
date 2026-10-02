//! Per-job credential broker on a Unix socket (Linux transport, DG-BROKER).
//!
//! Lifecycle: `start_broker` binds `<private runtime dir>/job-<id>.sock` (mode 0600),
//! the caller spawns Git and calls `bind_peer(git_pid)` while it still owns the child,
//! and the broker serves only that process tree until the grant expires or the handle
//! is shut down/dropped, at which point the socket file is removed.
//!
//! Caller contract (P05 Git runner): shut down or drop the handle as soon as the Git
//! child exits, before reaping it. The broker also refuses requests once the bound
//! process has exited or become a zombie, so a recycled PID is never trusted. A helper
//! that asks before `bind_peer` gets no credential (fail closed); the runner binds
//! immediately after `spawn` and before writing to Git's stdin.
//!
//! Any program in the bound Git process tree can ask for this job's credential, which
//! is why `git_auth_args` also disables hooks, fsmonitor, askpass and non-HTTPS
//! transports for every authenticated Git invocation.

use std::fmt;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gbm_domain::contract::schema::is_id;
use gbm_domain::contract::secret::SecretString;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, oneshot};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{Instant, timeout};

use crate::platform::{PrivateRuntimeDir, current_uid, is_running, is_same_or_descendant};

use super::protocol::{Attributes, MAX_MESSAGE_BYTES, is_line_safe};
use super::remote::RemoteTarget;

/// Longest grant a caller may request; one job's network phase must fit in it.
pub const MAX_GRANT_TTL: Duration = Duration::from_secs(60 * 60);
/// Credentials served per grant; Git asks once per connection, retries are bounded.
const MAX_SERVED: usize = 32;
/// Git's helper writes its tiny request at once; a slower in-tree client is dropped.
const READ_TIMEOUT: Duration = Duration::from_secs(1);
/// Concurrent requests from the job's own process tree.
const MAX_IN_FLIGHT: usize = 4;
/// `sockaddr_un.sun_path` is 108 bytes including the terminating NUL.
const MAX_SOCKET_PATH_BYTES: usize = 107;
const MAX_DENIALS_RECORDED: usize = 64;

/// What a job may receive: one username/secret for one repository on one host.
pub struct JobGrant {
    pub job_id: String,
    pub target: RemoteTarget,
    pub username: String,
    pub secret: SecretString,
    pub ttl: Duration,
}

impl fmt::Debug for JobGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobGrant")
            .field("job_id", &self.job_id)
            .field("target", &self.target)
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    /// Invalid job ID, username/secret not protocol-safe, or TTL out of range.
    InvalidGrant,
    /// A socket for this job already exists; the broker never reuses or unlinks it.
    AlreadyRunning,
    /// The runtime directory path is too long for a Unix socket address.
    SocketPathTooLong,
    Platform,
    Io,
}

impl fmt::Display for BrokerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidGrant => "invalid credential grant",
            Self::AlreadyRunning => "a credential broker for this job already exists",
            Self::SocketPathTooLong => "the runtime directory path is too long for a socket",
            Self::Platform => "the credential broker is not supported here",
            Self::Io => "the credential broker could not be started",
        })
    }
}

impl std::error::Error for BrokerError {}

/// Why a request received no credential. Recorded for diagnostics; carries no secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denial {
    PeerCredentialsUnavailable,
    PeerUidMismatch,
    /// `bind_peer` has not been called yet.
    PeerNotBound,
    PeerNotInJobProcessTree,
    /// The bound Git process has exited (or is a zombie); its PID is no longer trusted.
    JobProcessExited,
    Malformed,
    UnsupportedOperation,
    ProtocolNotHttps,
    HostMismatch,
    PathMismatch,
    LimitReached,
}

struct Shared {
    peer: Mutex<Option<u32>>,
    denials: Mutex<Vec<Denial>>,
    served: Mutex<usize>,
}

impl Shared {
    fn deny(&self, denial: Denial) {
        if let Ok(mut denials) = self.denials.lock()
            && denials.len() < MAX_DENIALS_RECORDED
        {
            denials.push(denial);
        }
    }
}

/// Owner of a running broker. Dropping it stops the broker and removes the socket.
pub struct BrokerHandle {
    socket_path: PathBuf,
    socket_id: (u64, u64),
    shared: Arc<Shared>,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl fmt::Debug for BrokerHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BrokerHandle")
            .field("socket_path", &self.socket_path)
            .finish_non_exhaustive()
    }
}

impl BrokerHandle {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Binds the broker to the job's Git process. Call with the PID of a child the
    /// caller has spawned and not yet reaped, so the PID cannot be recycled meanwhile.
    pub fn bind_peer(&self, git_pid: u32) {
        if let Ok(mut peer) = self.shared.peer.lock() {
            *peer = Some(git_pid);
        }
    }

    pub fn denials(&self) -> Vec<Denial> {
        self.shared
            .denials
            .lock()
            .map(|d| d.clone())
            .unwrap_or_default()
    }

    pub fn served(&self) -> usize {
        self.shared.served.lock().map(|s| *s).unwrap_or(0)
    }

    /// True once the broker stopped (expired or shut down) and removed its socket.
    pub fn is_finished(&self) -> bool {
        self.task.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Stops serving and waits until the socket file is removed.
    pub async fn shutdown(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        remove_if_same(&self.socket_path, self.socket_id);
    }
}

impl Drop for BrokerHandle {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        remove_if_same(&self.socket_path, self.socket_id);
    }
}

/// Removes `job-*.sock` files left by a crashed broker (nothing listens on them).
/// Call once at startup, before any broker is started. Returns how many were removed.
pub fn sweep_stale_sockets(dir: &PrivateRuntimeDir) -> usize {
    let Ok(entries) = fs::read_dir(dir.path()) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let is_job_socket = name
            .to_str()
            .is_some_and(|n| n.starts_with("job-") && n.ends_with(".sock"))
            && entry.file_type().is_ok_and(|t| t.is_socket());
        if is_job_socket
            && std::os::unix::net::UnixStream::connect(&path)
                .is_err_and(|e| e.kind() == std::io::ErrorKind::ConnectionRefused)
            && fs::remove_file(&path).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

fn validate(grant: &JobGrant) -> Result<(), BrokerError> {
    let ok = is_id(&grant.job_id)
        && grant.job_id.len() <= 64
        && is_line_safe(&grant.username)
        && is_line_safe(grant.secret.expose())
        && !grant.ttl.is_zero()
        && grant.ttl <= MAX_GRANT_TTL;
    if ok {
        Ok(())
    } else {
        Err(BrokerError::InvalidGrant)
    }
}

/// Starts a broker for one job inside `dir`. Must run inside a Tokio runtime.
pub async fn start_broker(
    dir: &PrivateRuntimeDir,
    grant: JobGrant,
) -> Result<BrokerHandle, BrokerError> {
    validate(&grant)?;
    let uid = current_uid().map_err(|_| BrokerError::Platform)?;
    let socket_path = dir.path().join(format!("job-{}.sock", grant.job_id));
    if socket_path.as_os_str().as_bytes().len() > MAX_SOCKET_PATH_BYTES {
        return Err(BrokerError::SocketPathTooLong);
    }
    if fs::symlink_metadata(&socket_path).is_ok() {
        return Err(BrokerError::AlreadyRunning);
    }
    let listener = UnixListener::bind(&socket_path).map_err(|_| BrokerError::Io)?;
    if fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600)).is_err() {
        let _ = fs::remove_file(&socket_path);
        return Err(BrokerError::Io);
    }
    let Some(socket_id) = file_id(&socket_path) else {
        return Err(BrokerError::Io);
    };
    let shared = Arc::new(Shared {
        peer: Mutex::new(None),
        denials: Mutex::new(Vec::new()),
        served: Mutex::new(0),
    });
    let (stop_tx, stop_rx) = oneshot::channel();
    let deadline = Instant::now() + grant.ttl;
    let task = tokio::spawn(serve(Server {
        listener,
        socket_path: socket_path.clone(),
        socket_id,
        grant,
        uid,
        shared: Arc::clone(&shared),
        deadline,
        stop: stop_rx,
    }));
    Ok(BrokerHandle {
        socket_path,
        socket_id,
        shared,
        stop: Some(stop_tx),
        task: Some(task),
    })
}

/// Everything the serving task owns.
struct Server {
    listener: UnixListener,
    socket_path: PathBuf,
    socket_id: (u64, u64),
    grant: JobGrant,
    uid: u32,
    shared: Arc<Shared>,
    deadline: Instant,
    stop: oneshot::Receiver<()>,
}

async fn serve(server: Server) {
    let Server {
        listener,
        socket_path,
        socket_id,
        grant,
        uid,
        shared,
        deadline,
        mut stop,
    } = server;
    let grant = Arc::new(grant);
    let permits = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
    // Owned request tasks: dropped (aborted) together with this loop.
    let mut in_flight = JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stop => break,
            () = tokio::time::sleep_until(deadline) => break,
            Some(_) = in_flight.join_next(), if !in_flight.is_empty() => {}
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                // Peer identity is checked before reading anything, so a foreign or
                // silent client cannot occupy the broker.
                if let Err(denial) = check_peer(&stream, uid, &shared) {
                    shared.deny(denial);
                    continue;
                }
                let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                    shared.deny(Denial::LimitReached);
                    continue;
                };
                let (grant, shared) = (Arc::clone(&grant), Arc::clone(&shared));
                in_flight.spawn(async move {
                    let _ = timeout(READ_TIMEOUT, handle(stream, &grant, &shared)).await;
                    drop(permit);
                });
            }
        }
    }
    drop(listener);
    remove_if_same(&socket_path, socket_id);
}

/// Identity of the socket file this broker created (device, inode).
fn file_id(path: &Path) -> Option<(u64, u64)> {
    fs::symlink_metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

/// Unlinks `path` only if it is still the socket this broker created, so an old
/// handle never removes a newer broker's socket for the same job.
fn remove_if_same(path: &Path, id: (u64, u64)) {
    if file_id(path) == Some(id) {
        let _ = fs::remove_file(path);
    }
}

fn check_peer(stream: &UnixStream, uid: u32, shared: &Shared) -> Result<(), Denial> {
    let cred = stream
        .peer_cred()
        .map_err(|_| Denial::PeerCredentialsUnavailable)?;
    if cred.uid() != uid {
        return Err(Denial::PeerUidMismatch);
    }
    let bound = shared
        .peer
        .lock()
        .ok()
        .and_then(|p| *p)
        .ok_or(Denial::PeerNotBound)?;
    if !is_running(bound) {
        return Err(Denial::JobProcessExited);
    }
    let peer_pid = cred
        .pid()
        .and_then(|p| u32::try_from(p).ok())
        .ok_or(Denial::PeerCredentialsUnavailable)?;
    if !is_same_or_descendant(peer_pid, bound) {
        return Err(Denial::PeerNotInJobProcessTree);
    }
    Ok(())
}

async fn handle(mut stream: UnixStream, grant: &JobGrant, shared: &Shared) {
    let response = match authorize(&mut stream, grant, shared).await {
        Ok(()) => {
            let mut answer = Attributes::default();
            answer.set("username", &grant.username);
            answer.set("password", grant.secret.expose());
            answer.to_message()
        }
        Err(denial) => {
            shared.deny(denial);
            "\n".to_owned()
        }
    };
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

async fn authorize(
    stream: &mut UnixStream,
    grant: &JobGrant,
    shared: &Shared,
) -> Result<(), Denial> {
    let mut buffer = Vec::with_capacity(512);
    let read = (&mut *stream)
        .take(MAX_MESSAGE_BYTES as u64 + 1)
        .read_to_end(&mut buffer)
        .await;
    if read.is_err() || buffer.len() > MAX_MESSAGE_BYTES {
        return Err(Denial::Malformed);
    }
    let request = Attributes::parse(&buffer).map_err(|_| Denial::Malformed)?;
    if request.get("operation") != Some("get") {
        return Err(Denial::UnsupportedOperation);
    }
    if request.get("protocol") != Some("https") {
        return Err(Denial::ProtocolNotHttps);
    }
    if request.get("host") != Some(grant.target.host().as_str()) {
        return Err(Denial::HostMismatch);
    }
    if request.get("path") != Some(grant.target.path()) {
        return Err(Denial::PathMismatch);
    }
    let mut served = shared.served.lock().map_err(|_| Denial::LimitReached)?;
    if *served >= MAX_SERVED {
        return Err(Denial::LimitReached);
    }
    *served += 1;
    Ok(())
}
