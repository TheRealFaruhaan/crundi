//! The ops area (AREAS.md): phase 5: Services, forwards, Docker, Secrets, Skills, Usage, Settings, Homepool, Info, Panels, What's new.
//!
//! The registry (src/areas.rs) calls the functions below; keep their
//! signatures. Each screen has its own module (src/ops/<screen>.rs, with the
//! same functions) and this file fans every call out to all of them. Server
//! calls go through crate::kit (kit::call, never on the UI thread), the
//! shared dialogs through kit::confirm / kit::prompt / kit::menu.

use crate::{areas::AreaEvent, MainWindow};
use serde_json::{json, Map, Value};

pub mod services;
pub mod docker;
pub mod homepool;
pub mod info;
pub mod vault;
pub mod settings;
pub mod skills;
pub mod panels;
pub mod extras;

macro_rules! each { ($f:ident ( $($a:expr),* )) => { services::$f($($a),*); docker::$f($($a),*); homepool::$f($($a),*); info::$f($($a),*); vault::$f($($a),*); settings::$f($($a),*); skills::$f($($a),*); panels::$f($($a),*); extras::$f($($a),*); }; }

pub fn init(w: &MainWindow) { each!(init(w)); }

pub fn signed_in() { each!(signed_in()); }

pub fn signed_out() { each!(signed_out()); }

pub fn project(alias: &str) { each!(project(alias)); }

pub fn tab_shown(tab: &str) { each!(tab_shown(tab)); }

pub fn event(e: &AreaEvent) { each!(event(e)); }

/// The test driver's "ops:<name>" actions; Some(answer) for a known one.
pub fn test_ui(name: &str, args: &[String]) -> Option<Value> {
    if name == "ping" { return Some(json!({ "ok": true, "area": "ops" })); }
    None
        .or_else(|| services::test_ui(name, args))
        .or_else(|| docker::test_ui(name, args))
        .or_else(|| homepool::test_ui(name, args))
        .or_else(|| info::test_ui(name, args))
        .or_else(|| vault::test_ui(name, args))
        .or_else(|| settings::test_ui(name, args))
        .or_else(|| skills::test_ui(name, args))
        .or_else(|| panels::test_ui(name, args))
        .or_else(|| extras::test_ui(name, args))
}

/// What the test driver sees of the area ({"cmd":"area","area":"ops"}): one key per screen.
pub fn props() -> Value {
    let mut o = Map::new();
    o.insert("services".into(), services::props());
    o.insert("docker".into(), docker::props());
    o.insert("homepool".into(), homepool::props());
    o.insert("info".into(), info::props());
    o.insert("vault".into(), vault::props());
    o.insert("settings".into(), settings::props());
    o.insert("skills".into(), skills::props());
    o.insert("panels".into(), panels::props());
    o.insert("extras".into(), extras::props());
    Value::Object(o)
}
