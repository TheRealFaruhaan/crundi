//! crundi-worker: joins a PC to a Crundi server and keeps a link to it.
//! The contract is `specs/workers-protocol.md` (phase 1: join, the link,
//! info and labels, pause, the PC terminal).

pub mod app;
pub mod config;
pub mod info;
pub mod join;
pub mod link;
pub mod service;
pub mod setup;
pub mod term;
pub mod util;

#[cfg(feature = "tray")]
pub mod tray;
