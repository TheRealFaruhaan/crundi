//! `crundi-worker mcp --chat <c> --port <p> --token <t>`: the chat's MCP
//! server (JSON-RPC 2.0, one message per line on stdio, protocol
//! 2024-11-05). tools/list and tools/call go to the running worker's
//! loopback relay (127.0.0.1:<p>), which sends them to the server over the
//! link. Results are wrapped as MCP content the way src/mcp-stdio.js does.
//!
//! Relay protocol (lines of JSON): first `{"token","chat"}` -> `{"ok":true}`,
//! then `{"id",op:"list"}` / `{"id",op:"call",tool,args}` ->
//! `{"id",result}` | `{"id",error}`.

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

pub const PROTOCOL_VERSION: &str = "2024-11-05";
/// Tools whose `{ok, data}` answer is a PNG (as in mcp-stdio.js).
const IMAGE_TOOLS: &[&str] = &["browser_screenshot", "capture_window", "capture_display"];

/// A tool's JSON answer as MCP CallTool content (mcp-stdio.js, handleToolCall's end).
pub fn wrap_result(tool: &str, result: &Value) -> Value {
    let ok = result.get("ok").and_then(Value::as_bool) == Some(true);
    if ok {
        if let Some(c) = result.get("content").filter(|c| c.is_array()) {
            return json!({"content": c});
        }
    }
    if IMAGE_TOOLS.contains(&tool) && ok {
        if let Some(d) = result.get("data").and_then(Value::as_str).filter(|d| !d.is_empty()) {
            return json!({"content": [{"type": "image", "data": d, "mimeType": "image/png"}]});
        }
    }
    let err = result.get("error").filter(|e| !e.is_null() && *e != "" && *e != false);
    if !ok {
        if let Some(e) = err {
            let text = e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string());
            return json!({"content": [{"type": "text", "text": format!("Error: {text}")}], "isError": true});
        }
    }
    let text = serde_json::to_string_pretty(result).unwrap_or_default();
    json!({"content": [{"type": "text", "text": text}]})
}

struct Conn {
    tx: mpsc::UnboundedSender<String>,
    alive: Arc<AtomicBool>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
}

pub struct Relay {
    port: u16,
    token: String,
    chat: String,
    conn: tokio::sync::Mutex<Option<Conn>>,
    next: AtomicU64,
}

impl Relay {
    pub fn new(chat: &str, port: u16, token: &str) -> Relay {
        Relay { port, token: token.into(), chat: chat.into(), conn: Default::default(), next: AtomicU64::new(1) }
    }

    async fn connect(&self) -> Result<Conn> {
        let sock = tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(("127.0.0.1", self.port)))
            .await
            .map_err(|_| anyhow!("timed out reaching the worker"))??;
        let (r, mut w) = sock.into_split();
        w.write_all(format!("{}\n", json!({"token": self.token, "chat": self.chat})).as_bytes()).await?;
        let mut lines = tokio::io::BufReader::new(r).lines();
        let first = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
            .await
            .map_err(|_| anyhow!("the worker did not answer"))??
            .ok_or_else(|| anyhow!("the worker closed the relay"))?;
        let v: Value = serde_json::from_str(&first)?;
        if v.get("ok").and_then(Value::as_bool) != Some(true) {
            bail!("the worker refused the relay: {}", v.get("error").and_then(Value::as_str).unwrap_or("?"));
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let alive = Arc::new(AtomicBool::new(true));
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Default::default();
        tokio::spawn(async move {
            while let Some(l) = rx.recv().await {
                if w.write_all(l.as_bytes()).await.is_err() {
                    break;
                }
            }
        });
        let (a, p) = (alive.clone(), pending.clone());
        tokio::spawn(async move {
            while let Ok(Some(l)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&l) else { continue };
                let Some(id) = v.get("id").and_then(Value::as_u64) else { continue };
                if let Some(tx) = p.lock().unwrap().remove(&id) {
                    let _ = tx.send(v);
                }
            }
            a.store(false, Ordering::SeqCst);
            p.lock().unwrap().clear(); // waiting calls fail
        });
        Ok(Conn { tx, alive, pending })
    }

    /// `op` is `{"op":"list"}` or `{"op":"call","tool","args"}`; returns the worker's `result`.
    pub async fn request(&self, mut op: Value) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        op["id"] = json!(id);
        let rx = {
            let mut c = self.conn.lock().await;
            if c.as_ref().is_none_or(|c| !c.alive.load(Ordering::SeqCst)) {
                *c = Some(self.connect().await.map_err(|e| format!("the worker is not reachable: {e:#}"))?);
            }
            let c = c.as_ref().unwrap();
            let (tx, rx) = oneshot::channel();
            c.pending.lock().unwrap().insert(id, tx);
            if c.tx.send(format!("{op}\n")).is_err() {
                return Err("the worker is not reachable".into());
            }
            rx
        };
        let v = rx.await.map_err(|_| "the worker closed the relay".to_string())?;
        match v.get("error") {
            Some(e) if !e.is_null() => Err(e.as_str().map(str::to_string).unwrap_or_else(|| e.to_string())),
            _ => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
        }
    }
}

fn rpc_result(id: &Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn rpc_error(id: &Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}

/// Answer one JSON-RPC message; None for notifications.
pub async fn answer(relay: &Relay, msg: &Value) -> Option<String> {
    let id = msg.get("id").filter(|i| !i.is_null())?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    Some(match method {
        "initialize" => rpc_result(
            &id,
            json!({"protocolVersion": PROTOCOL_VERSION, "capabilities": {"tools": {}},
                "serverInfo": {"name": "crundi", "version": crate::util::VERSION}}),
        ),
        "ping" => rpc_result(&id, json!({})),
        "tools/list" => match relay.request(json!({"op": "list"})).await {
            Ok(r) => rpc_result(&id, json!({"tools": r.get("tools").cloned().unwrap_or(json!([]))})),
            Err(e) => rpc_error(&id, -32603, &e),
        },
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let args = params.get("arguments").cloned().filter(|a| a.is_object()).unwrap_or(json!({}));
            let res = match relay.request(json!({"op": "call", "tool": name, "args": args})).await {
                Ok(r) => wrap_result(&name, &r),
                Err(e) => json!({"content": [{"type": "text", "text": format!("Error calling {name}: {e}")}], "isError": true}),
            };
            rpc_result(&id, res)
        }
        _ => rpc_error(&id, -32601, &format!("Method not found: {method}")),
    })
}

/// Serve MCP on stdin/stdout until stdin closes.
pub async fn serve(chat: &str, port: u16, token: &str) -> i32 {
    let relay = Arc::new(Relay::new(chat, port, token));
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(l) = rx.recv().await {
            if out.write_all(format!("{l}\n").as_bytes()).await.is_err() || out.flush().await.is_err() {
                break;
            }
        }
    });
    let mut tasks = tokio::task::JoinSet::new();
    let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                let _ = tx.send(rpc_error(&Value::Null, -32700, "Parse error"));
                continue;
            }
        };
        let msgs = match msg {
            Value::Array(a) => a,
            m => vec![m],
        };
        for m in msgs {
            let (relay, tx) = (relay.clone(), tx.clone());
            tasks.spawn(async move {
                if let Some(a) = answer(&relay, &m).await {
                    let _ = tx.send(a);
                }
            });
        }
    }
    while tasks.join_next().await.is_some() {}
    drop(tx);
    let _ = writer.await;
    0
}
