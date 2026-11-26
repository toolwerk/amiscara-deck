//! The connection of a UI to the daemon: take the full state, then every
//! change, and send answers. The command line and (through `deck-ffi`) the
//! menu bar app use this.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::Agent;
use deck_proto::message::{
    AnswerVia, DaemonMessage, Decision, Settings, UiMessage, decode_line, encode_line,
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

/// A line from the daemon is a state of a few agents; this is far above that.
const MAX_LINE: u64 = 16 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No daemon at the socket: not started, or started in another folder.
    #[error("no daemon at {0}")]
    NoDaemon(PathBuf),
    #[error("the daemon speaks protocol {daemon}, this program {ours}")]
    VersionMismatch { daemon: u32, ours: u32 },
    #[error("the daemon sent something unexpected: {0}")]
    Unexpected(String),
    #[error("the connection ended")]
    Closed,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// What a UI knows: the state of this daemon.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub host: String,
    pub settings: Settings,
    pub agents: BTreeMap<String, Agent>,
    /// What today cost, in millionths of a US dollar (an estimate).
    pub today_micro_usd: Option<u64>,
    /// The fingerprint of the running daemon; `None` from an older one.
    pub build: Option<String>,
}

impl State {
    /// Apply a change message. Other messages leave the state alone.
    pub fn apply(&mut self, msg: &DaemonMessage) {
        match msg {
            DaemonMessage::AgentChanged(a) => {
                self.agents.insert(a.id.clone(), (**a).clone());
            }
            DaemonMessage::AgentRemoved { id } => {
                self.agents.remove(id);
            }
            DaemonMessage::SettingsChanged(s) => self.settings = s.clone(),
            DaemonMessage::Costs { today_micro_usd } => {
                self.today_micro_usd = Some(*today_micro_usd);
            }
            DaemonMessage::Welcome { .. }
            | DaemonMessage::VersionMismatch { .. }
            | DaemonMessage::Notify { .. }
            | DaemonMessage::Withdraw { .. }
            | DaemonMessage::Unknown => {}
        }
    }
}

type Reader = BufReader<Box<dyn AsyncRead + Send + Unpin>>;
type Writer = Box<dyn AsyncWrite + Send + Unpin>;

pub struct Client {
    reader: Reader,
    /// The part of a line read so far. It lives here and not in `next`, so
    /// `next` can be cancelled (as in `tokio::select!`) without losing it.
    partial: Vec<u8>,
    writer: Writer,
    state: State,
}

impl Client {
    /// Connect, say hello and take the full state.
    pub async fn connect(socket: &Path, name: &str) -> Result<Self, Error> {
        let stream = match tokio::time::timeout(CONNECT_TIMEOUT, deck_ipc::connect(socket)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Err(Error::NoDaemon(socket.to_owned()));
            }
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => return Err(Error::NoDaemon(socket.to_owned())),
        };
        Self::over(stream, name).await
    }

    /// Connect over any stream that carries the protocol: the local socket or
    /// pipe, or a channel of an SSH connection that leads to the socket of a
    /// daemon on another machine. Say hello and take the full state.
    pub async fn over<S>(stream: S, name: &str) -> Result<Self, Error>
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let (r, w) = tokio::io::split(stream);
        let r: Box<dyn AsyncRead + Send + Unpin> = Box::new(r);
        let mut writer: Writer = Box::new(w);
        let hello = UiMessage::Hello {
            protocol: PROTOCOL_VERSION,
            name: name.to_owned(),
        };
        writer
            .write_all(
                encode_line(&hello)
                    .map_err(|e| Error::Unexpected(e.to_string()))?
                    .as_bytes(),
            )
            .await?;
        let mut reader = BufReader::new(r);
        let mut partial = Vec::new();
        match read_message(&mut reader, &mut partial).await? {
            DaemonMessage::Welcome {
                host,
                settings,
                agents,
                today_micro_usd,
                build,
                ..
            } => Ok(Client {
                reader,
                partial,
                writer,
                state: State {
                    host,
                    settings,
                    agents: agents.into_iter().map(|a| (a.id.clone(), a)).collect(),
                    today_micro_usd,
                    build,
                },
            }),
            DaemonMessage::VersionMismatch { daemon, ui } => {
                Err(Error::VersionMismatch { daemon, ours: ui })
            }
            other => Err(Error::Unexpected(format!("{other:?}"))),
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    /// The next message of the daemon, already applied to the state.
    /// `Error::Closed` when the daemon goes away. Cancel safe: a line read
    /// only in part is finished by the next call.
    pub async fn next(&mut self) -> Result<DaemonMessage, Error> {
        let msg = read_message(&mut self.reader, &mut self.partial).await?;
        self.state.apply(&msg);
        Ok(msg)
    }

    /// Answer a held request.
    pub async fn answer(
        &mut self,
        request_id: &str,
        decision: Decision,
        via: AnswerVia,
    ) -> Result<(), Error> {
        self.send(&UiMessage::Answer {
            request_id: request_id.to_owned(),
            decision,
            via,
        })
        .await
    }

    pub async fn send(&mut self, msg: &UiMessage) -> Result<(), Error> {
        let line = encode_line(msg).map_err(|e| Error::Unexpected(e.to_string()))?;
        self.writer.write_all(line.as_bytes()).await?;
        Ok(())
    }
}

/// Read one line into `buf`, which keeps what was read when the future is
/// dropped half way: `read_until` hands over bytes as it consumes them.
async fn read_message(reader: &mut Reader, buf: &mut Vec<u8>) -> Result<DaemonMessage, Error> {
    let limit = MAX_LINE.saturating_sub(buf.len() as u64);
    (&mut *reader).take(limit).read_until(b'\n', buf).await?;
    // No newline: the daemon went away, or the line is too long.
    if buf.last() != Some(&b'\n') {
        buf.clear();
        return Err(Error::Closed);
    }
    let line =
        String::from_utf8(std::mem::take(buf)).map_err(|e| Error::Unexpected(e.to_string()))?;
    decode_line(&line).map_err(|e| Error::Unexpected(e.to_string()))
}

/// The state once, without staying connected.
pub async fn fetch(socket: &Path, name: &str) -> Result<State, Error> {
    Ok(Client::connect(socket, name).await?.state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader(r: tokio::io::DuplexStream) -> Reader {
        BufReader::new(Box::new(r))
    }

    #[tokio::test]
    async fn a_line_read_in_part_survives_a_cancelled_read() {
        let (r, mut w) = tokio::io::duplex(1024);
        let mut reader = reader(r);
        let mut partial = Vec::new();
        let line = encode_line(&DaemonMessage::AgentRemoved {
            id: "a1".to_owned(),
        })
        .expect("encode");
        let (head, tail) = line.split_at(line.len() / 2);

        w.write_all(head.as_bytes()).await.expect("write");
        // As in `tokio::select!` when a command comes first.
        let cut = tokio::time::timeout(
            Duration::from_millis(50),
            read_message(&mut reader, &mut partial),
        )
        .await;
        assert!(cut.is_err(), "no whole line yet");
        assert_eq!(partial, head.as_bytes());

        w.write_all(tail.as_bytes()).await.expect("write");
        let msg = read_message(&mut reader, &mut partial).await.expect("read");
        assert!(matches!(msg, DaemonMessage::AgentRemoved { id } if id == "a1"));
        assert!(partial.is_empty());
    }

    #[tokio::test]
    async fn a_close_in_the_middle_of_a_line_is_closed() {
        let (r, mut w) = tokio::io::duplex(1024);
        let mut reader = reader(r);
        let mut partial = Vec::new();
        w.write_all(b"{\"type\":").await.expect("write");
        drop(w);
        assert!(matches!(
            read_message(&mut reader, &mut partial).await,
            Err(Error::Closed)
        ));
    }
}
