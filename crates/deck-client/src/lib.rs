//! The connection of a UI to the daemon: take the full state, then every
//! change, and send answers. The command line and (through `deck-ffi`) the
//! menu bar app use this.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::Agent;
use deck_proto::message::{
    AnswerVia, DaemonMessage, Decision, Settings, UiMessage, decode_line, encode_line,
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

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
    writer: Writer,
    state: State,
}

impl Client {
    /// Connect, say hello and take the full state.
    pub async fn connect(socket: &Path, name: &str) -> Result<Self, Error> {
        let stream = match tokio::time::timeout(CONNECT_TIMEOUT, UnixStream::connect(socket)).await
        {
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

    /// Connect over any stream that carries the protocol: a Unix socket, or a
    /// channel of an SSH connection that leads to the socket of a daemon on
    /// another machine. Say hello and take the full state.
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
        match read_message(&mut reader).await? {
            DaemonMessage::Welcome {
                host,
                settings,
                agents,
                today_micro_usd,
                build,
                ..
            } => Ok(Client {
                reader,
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
    /// `Error::Closed` when the daemon goes away.
    pub async fn next(&mut self) -> Result<DaemonMessage, Error> {
        let msg = read_message(&mut self.reader).await?;
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

async fn read_message(reader: &mut Reader) -> Result<DaemonMessage, Error> {
    let mut buf = Vec::new();
    let n = reader.take(MAX_LINE).read_until(b'\n', &mut buf).await?;
    if n == 0 || buf.last() != Some(&b'\n') {
        return Err(Error::Closed);
    }
    let line = String::from_utf8(buf).map_err(|e| Error::Unexpected(e.to_string()))?;
    decode_line(&line).map_err(|e| Error::Unexpected(e.to_string()))
}

/// The state once, without staying connected.
pub async fn fetch(socket: &Path, name: &str) -> Result<State, Error> {
    Ok(Client::connect(socket, name).await?.state)
}
