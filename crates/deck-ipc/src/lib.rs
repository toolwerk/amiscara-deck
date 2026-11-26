//! The local connection between the daemon and its clients (UIs, the
//! simulator, the command line).
//!
//! Everyone names the daemon by its socket path in the data folder. On Unix
//! that is a Unix socket; the folder (0700) and the socket (0600) keep other
//! users out. On Windows the daemon listens on a named pipe with a random
//! name and writes that name into the socket path
//! ([`deck_proto::paths::new_pipe_name`]); the data folder keeps the name
//! from other users, and the pipe's access list admits only its owner (the
//! user who started the daemon) and the system. Remote clients are refused.
//!
//! The hook program does not use this crate: it stays without tokio and
//! opens the socket or pipe with std.

use std::io;
use std::path::Path;

#[cfg(unix)]
mod imp {
    use std::io;
    use std::path::Path;

    pub type Stream = tokio::net::UnixStream;

    pub async fn connect(socket: &Path) -> io::Result<Stream> {
        tokio::net::UnixStream::connect(socket).await
    }

    pub struct Listener(tokio::net::UnixListener);

    impl Listener {
        pub fn bind(socket: &Path) -> io::Result<Self> {
            tokio::net::UnixListener::bind(socket).map(Listener)
        }

        pub async fn accept(&self) -> io::Result<Stream> {
            self.0.accept().await.map(|(s, _)| s)
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::path::Path;

    use interprocess::local_socket::tokio::{Listener as PipeListener, Stream as PipeStream};
    use interprocess::local_socket::traits::tokio::{Listener as _, Stream as _};
    use interprocess::local_socket::{GenericFilePath, ListenerOptions, ToFsName};
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;

    pub type Stream = PipeStream;

    /// Full access for the owner of the pipe (the user who started the
    /// daemon) and the system, nobody else. `P`: no inherited entries.
    const ACCESS: &str = "D:P(A;;GA;;;OW)(A;;GA;;;SY)";

    /// The name comes from the file the daemon wrote; a missing file is
    /// `NotFound`, as a missing socket is on Unix.
    pub async fn connect(socket: &Path) -> io::Result<Stream> {
        let name = deck_proto::paths::read_pipe_name(socket)?;
        PipeStream::connect(name.as_str().to_fs_name::<GenericFilePath>()?).await
    }

    pub struct Listener(PipeListener);

    impl Listener {
        /// Listen on a new random pipe, then publish its name in `socket`.
        /// The pipe exists before the name does, so nobody can take it in
        /// between; `FILE_FLAG_FIRST_PIPE_INSTANCE` makes creating it fail if
        /// someone already holds that name.
        pub fn bind(socket: &Path) -> io::Result<Self> {
            let name = deck_proto::paths::new_pipe_name();
            let access = widestring::U16CString::from_str(ACCESS)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
            let listener = ListenerOptions::new()
                .name(name.as_str().to_fs_name::<GenericFilePath>()?)
                .security_descriptor(SecurityDescriptor::deserialize(&access)?)
                .create_tokio()?;
            let tmp = socket.with_extension("tmp");
            std::fs::write(&tmp, format!("{name}\n"))?;
            std::fs::rename(&tmp, socket)?;
            Ok(Listener(listener))
        }

        pub async fn accept(&self) -> io::Result<Stream> {
            self.0.accept().await
        }
    }
}

/// A connection, read and written with tokio.
pub use imp::Stream;

/// The daemon's end: accepts one connection after the other.
pub use imp::Listener;

/// The read half of a [`Stream`].
pub type ReadHalf = tokio::io::ReadHalf<Stream>;
/// The write half of a [`Stream`].
pub type WriteHalf = tokio::io::WriteHalf<Stream>;

/// Connect to the daemon behind `socket`.
pub async fn connect(socket: &Path) -> io::Result<Stream> {
    imp::connect(socket).await
}

/// Read and write independently, for example from two tasks.
pub fn split(stream: Stream) -> (ReadHalf, WriteHalf) {
    tokio::io::split(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[tokio::test]
    async fn a_line_goes_there_and_back() {
        let dir = std::env::temp_dir().join(format!("deck-ipc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let socket = dir.join("t.sock");
        let listener = Listener::bind(&socket).expect("bind");
        let server = tokio::spawn(async move {
            let stream = listener.accept().await.expect("accept");
            let (read, mut write) = split(stream);
            let mut line = String::new();
            BufReader::new(read)
                .read_line(&mut line)
                .await
                .expect("read");
            write
                .write_all(line.to_uppercase().as_bytes())
                .await
                .expect("write");
            write.shutdown().await.expect("shutdown");
        });
        let stream = connect(&socket).await.expect("connect");
        let (read, mut write) = split(stream);
        write.write_all(b"hello\n").await.expect("write");
        let mut line = String::new();
        BufReader::new(read)
            .read_line(&mut line)
            .await
            .expect("read");
        assert_eq!(line, "HELLO\n");
        server.await.expect("server");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn no_daemon_is_an_error() {
        let socket = std::env::temp_dir().join("deck-ipc-nobody-here.sock");
        assert!(connect(&socket).await.is_err());
    }
}
