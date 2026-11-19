//! The client's non-UI core, with no GPUI (AREAS.md): the server's API and
//! sign-in flows, the live link, the saved login, and the rules every screen
//! uses. Ported from the Slint client (native-client/src) as it was: only
//! its ties to Slint were cut (the live link hands events to a callback; the
//! native host plugs in through `host`).

pub mod agents;
pub mod api;
pub mod attach;
pub mod chat;
pub mod drafts;
pub mod driver;
pub mod fresult;
pub mod head;
pub mod host;
pub mod live;
pub mod md;
pub mod media;
pub mod mosaic;
pub mod questions;
pub mod resume;
pub mod sched;
pub mod secrets;
pub mod slash;
pub mod sreq;
pub mod store;
pub mod think;
pub mod time;
pub mod tools;
pub mod voice;
pub mod voiceio;
