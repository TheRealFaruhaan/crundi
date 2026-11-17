//! What the app and its browser host share, and nothing that needs CEF or
//! GPUI: the app links this without libcef; the host (crates/browser-host)
//! links it with libcef.
//!
//! - `ipc`: the link's protocol (commands, events) and its two ends;
//! - `shm`: a view's frame segment (shared memory);
//! - `proc`: starting the host so that it dies with the app, its PID file,
//!   reaping what a crashed run left;
//! - `common`: devices, cache folders, cursor kinds;
//! - `gpu`: the GPU path (CRUNDI_BROWSER_GPU, Windows and macOS): reading
//!   CEF's shared texture into a frame segment, and handing it to the app.

pub mod common;
pub mod gpu;
pub mod ipc;
pub mod proc;
pub mod shm;
