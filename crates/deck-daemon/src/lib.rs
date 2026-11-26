//! Background service `amiscara-deckd`: socket, start and stop.
//!
//! One Unix socket (mode 0600, inside a folder with mode 0700) for the hook
//! program and the UIs. M1-05 takes the hook messages and feeds the state of
//! `deck-session`; streaming to UIs (M1-10) and holding requests (M1-11,
//! M1-12) follow.

#![cfg(unix)]

mod costs;
mod decisions;
mod git;
pub mod launchd;
mod server;
mod state;
pub mod systemd;
mod tmux;

pub use deck_proto::paths::default_data_dir;
pub use server::{Config, Daemon, Error};
