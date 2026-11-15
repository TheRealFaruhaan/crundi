//! The Media modal and viewer belong to the workspace area (AREAS.md). Kanban
//! and the Mind map open them through here; until workspace provides
//! `open_media_modal` / `open_media` (shared/requests.md), these answer
//! false and the caller falls back to the Media tab.

use serde_json::Value;

/// The web's openMediaModal(title, state). False: not available.
pub fn open_modal(_title: &str, _state: &Value) -> bool { false }

/// The web's openMediaLightbox(item). False: not available.
pub fn open_item(_item: &Value) -> bool { false }
