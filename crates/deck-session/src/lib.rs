//! State of all agents, as pure functions: an event goes in, the new state and
//! the changes come out. No clock, no files, no sockets. The daemon supplies
//! the time, the branch and the place of the agent with every event.

mod activity;
pub mod notify;
mod request;
mod store;

pub use request::MAX_SUMMARY;
pub use store::{
    AnswerError, AnswerKind, Answered, Change, DENY_MESSAGE, EXITED_VISIBLE_MS, HISTORY_MAX,
    Incoming, Snapshot, Store,
};
