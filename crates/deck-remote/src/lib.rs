//! A UI that talks to the daemon of another machine.
//!
//! The daemon opens no network port. Instead this crate signs in over SSH and
//! opens a `direct-streamlocal` channel to the Unix socket of the remote
//! daemon; the channel carries the same protocol as a local socket
//! (`deck_client::Client::over`). The remote machine is just another daemon
//! with its own name in the state.

use std::path::PathBuf;
use std::time::Duration;

use amiscara_ssh::{Session, SshError, SshTarget};
use deck_client::Client;
use deck_proto::paths::SOCKET_NAME;

pub use amiscara_ssh::{HostKeyPolicy, Identity};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ssh: {0}")]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Client(#[from] deck_client::Error),
    /// Neither the given nor the usual socket paths lead to a daemon.
    #[error("no Deck daemon found on the other machine (tried {0})")]
    NoDaemon(String),
    /// The SSH server there does not allow forwarding to a Unix socket.
    #[error("the SSH server does not allow forwarding to the Deck socket (tried {0})")]
    ForwardingProhibited(String),
}

/// The common reasons a connection fails, each with its own next step in the
/// app. Rare errors have none and keep their technical text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Signed in, but no daemon answers there.
    NoDaemon,
    /// The SSH server refuses forwarding to a Unix socket.
    ForwardingProhibited,
    /// The server refused every offered key.
    AuthFailed,
    /// The own key could not be read, or the SSH agent is not reachable.
    KeyUnusable,
    /// No answer in time: switched off, asleep, or a firewall drops packets.
    Timeout,
    /// The machine answers but nothing listens on the SSH port.
    Refused,
    /// Name not found, no route, or another network error.
    Unreachable,
}

impl Error {
    /// Which of the common failures this is, if any.
    pub fn failure(&self) -> Option<Failure> {
        match self {
            Error::NoDaemon(_) => Some(Failure::NoDaemon),
            Error::ForwardingProhibited(_) => Some(Failure::ForwardingProhibited),
            Error::Ssh(SshError::AuthFailed(_)) => Some(Failure::AuthFailed),
            Error::Ssh(SshError::Key(_)) => Some(Failure::KeyUnusable),
            Error::Ssh(SshError::Timeout(_)) => Some(Failure::Timeout),
            Error::Ssh(SshError::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                Some(Failure::Refused)
            }
            Error::Ssh(SshError::Io(_)) => Some(Failure::Unreachable),
            _ => None,
        }
    }
}

/// How long signing in may take. A machine that is switched off otherwise
/// keeps the app in "connecting" for about a minute.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// A connection to a daemon on another machine. The SSH connection lives as
/// long as this value does.
pub struct RemoteClient {
    pub client: Client,
    /// The socket path that worked.
    pub socket: String,
    _session: Session,
}

/// Keeps the SSH connection open while it is alive.
pub struct Keepalive(#[allow(dead_code)] Session);

impl RemoteClient {
    /// The client and what keeps its SSH connection open: drop the second
    /// and the first loses its channel.
    pub fn into_parts(self) -> (Client, Keepalive) {
        (self.client, Keepalive(self._session))
    }
}

/// Where the daemon of the other machine usually keeps its socket, for the
/// home folder `home` of the user there. macOS first, then the other systems.
pub fn candidate_sockets(home: &str) -> Vec<String> {
    let home = home.trim_end_matches('/');
    vec![
        format!("{home}/Library/Application Support/Amiscara Deck/{SOCKET_NAME}"),
        format!("{home}/.local/share/amiscara-deck/{SOCKET_NAME}"),
    ]
}

/// The home folder of the user on the other machine, over SFTP: the server
/// does not expand `~` in a socket path.
async fn remote_home(session: &Session) -> Result<String, SshError> {
    let sftp = session.open_sftp().await?;
    sftp.canonicalize(".")
        .await
        .map_err(|e| SshError::SftpUnavailable(e.to_string()))
}

/// Sign in, find the socket and say hello to the daemon there. `socket` is
/// the path on the other machine if the user set one; without it the usual
/// places are tried.
pub async fn connect(
    target: &SshTarget,
    socket: Option<&str>,
    name: &str,
) -> Result<RemoteClient, Error> {
    let session = tokio::time::timeout(CONNECT_TIMEOUT, Session::connect(target))
        .await
        .map_err(|_| {
            SshError::Timeout(format!("{}@{}:{}", target.user, target.host, target.port))
        })??;
    let paths = match socket {
        Some(p) => vec![p.to_owned()],
        None => candidate_sockets(&remote_home(&session).await?),
    };
    let mut prohibited = false;
    for path in &paths {
        let channel = match session.direct_streamlocal(path).await {
            Ok(channel) => channel,
            Err(e) => {
                prohibited |= is_prohibited(&e);
                continue;
            }
        };
        match Client::over(channel.into_stream(), name).await {
            Ok(client) => {
                return Ok(RemoteClient {
                    client,
                    socket: path.clone(),
                    _session: session,
                });
            }
            // The daemon is there but speaks another protocol: say so.
            Err(e @ deck_client::Error::VersionMismatch { .. }) => return Err(e.into()),
            Err(_) => continue,
        }
    }
    if prohibited {
        return Err(Error::ForwardingProhibited(paths.join(", ")));
    }
    Err(Error::NoDaemon(paths.join(", ")))
}

/// The server said no to the channel itself, not "nothing there".
fn is_prohibited(e: &SshError) -> bool {
    matches!(
        e,
        SshError::Ssh(russh::Error::ChannelOpenFailure(
            russh::ChannelOpenFailure::AdministrativelyProhibited
        ))
    )
}

/// For the settings: where the keys and known hosts of the user usually are.
pub fn default_known_hosts() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".ssh/known_hosts"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io;

    fn io(kind: io::ErrorKind) -> Error {
        Error::Ssh(SshError::Io(io::Error::from(kind)))
    }

    #[test]
    fn common_failures_are_named() {
        let cases = [
            (Error::NoDaemon("x".into()), Some(Failure::NoDaemon)),
            (
                Error::ForwardingProhibited("x".into()),
                Some(Failure::ForwardingProhibited),
            ),
            (
                Error::Ssh(SshError::AuthFailed("u@h".into())),
                Some(Failure::AuthFailed),
            ),
            (
                Error::Ssh(SshError::Key("agent".into())),
                Some(Failure::KeyUnusable),
            ),
            (
                Error::Ssh(SshError::Timeout("u@h:22".into())),
                Some(Failure::Timeout),
            ),
            (io(io::ErrorKind::ConnectionRefused), Some(Failure::Refused)),
            (io(io::ErrorKind::NotFound), Some(Failure::Unreachable)),
            (
                io(io::ErrorKind::HostUnreachable),
                Some(Failure::Unreachable),
            ),
            (Error::Ssh(SshError::Refused("pty".into())), None),
            (Error::Ssh(SshError::Cancelled("u@h".into())), None),
        ];
        for (error, want) in cases {
            assert_eq!(error.failure(), want, "{error}");
        }
    }

    #[test]
    fn a_prohibited_channel_is_told_apart_from_a_missing_socket() {
        let prohibited = SshError::Ssh(russh::Error::ChannelOpenFailure(
            russh::ChannelOpenFailure::AdministrativelyProhibited,
        ));
        let failed = SshError::Ssh(russh::Error::ChannelOpenFailure(
            russh::ChannelOpenFailure::ConnectFailed,
        ));
        assert!(is_prohibited(&prohibited));
        assert!(!is_prohibited(&failed));
    }

    #[test]
    fn the_usual_sockets_are_absolute_paths_under_the_home_folder() {
        let c = candidate_sockets("/Users/frank/");
        assert_eq!(
            c,
            vec![
                "/Users/frank/Library/Application Support/Amiscara Deck/deckd.sock".to_owned(),
                "/Users/frank/.local/share/amiscara-deck/deckd.sock".to_owned(),
            ]
        );
    }
}
