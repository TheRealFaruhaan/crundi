//! The server's browser control (the native host, phase 3): live.rs asks
//! here whether this client hosts browsers and hands the host's messages
//! over. Desktop's webview crate plugs its host in at start (`set`); without
//! one the client does not register as a host.

use serde_json::Value;
use std::sync::{mpsc, OnceLock};

pub trait NativeHost: Send + Sync {
    /// The register message for a new socket (None: this client does not host).
    fn registration(&self) -> Option<Value>;
    /// The socket is up (Some: where binary frames go) or down (None).
    fn set_binary_out(&self, tx: Option<mpsc::Sender<Vec<u8>>>);
    /// A native-host message from the server; answers go to `tx`.
    fn on_message(&self, text: &str, tx: mpsc::Sender<Value>, shown: &str);
    /// A binary frame (the drawer's net tunnel).
    fn on_binary(&self, b: Vec<u8>);
}

static HOST: OnceLock<Box<dyn NativeHost>> = OnceLock::new();

/// Plug the host in (once, at start).
pub fn set(host: Box<dyn NativeHost>) { let _ = HOST.set(host); }

pub fn registration() -> Option<Value> { HOST.get().and_then(|h| h.registration()) }
pub fn set_binary_out(tx: Option<mpsc::Sender<Vec<u8>>>) { if let Some(h) = HOST.get() { h.set_binary_out(tx); } }
pub fn on_message(text: &str, tx: mpsc::Sender<Value>, shown: &str) { if let Some(h) = HOST.get() { h.on_message(text, tx, shown); } }
pub fn on_binary(b: Vec<u8>) { if let Some(h) = HOST.get() { h.on_binary(b); } }
