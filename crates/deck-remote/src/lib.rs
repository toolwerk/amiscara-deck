//! A UI that talks to the daemon of another machine.
//!
//! The daemon opens no network port. Instead this crate signs in over SSH and
//! opens a `direct-streamlocal` channel to the Unix socket of the remote
//! daemon; the channel carries the same protocol as a local socket
//! (`deck_client::Client::over`). The remote machine is just another daemon
//! with its own name in the state.

use std::path::PathBuf;

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
}

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
    let session = Session::connect(target).await?;
    let paths = match socket {
        Some(p) => vec![p.to_owned()],
        None => candidate_sockets(&remote_home(&session).await?),
    };
    for path in &paths {
        let Ok(channel) = session.direct_streamlocal(path).await else {
            continue;
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
    Err(Error::NoDaemon(paths.join(", ")))
}

/// For the settings: where the keys and known hosts of the user usually are.
pub fn default_known_hosts() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".ssh/known_hosts"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
