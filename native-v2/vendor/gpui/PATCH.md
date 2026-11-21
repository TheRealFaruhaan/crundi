# gpui-pre 0.3.12, patched for native-v2

This is the published `gpui-pre` 0.3.12 (crates.io; a snapshot of Zed's gpui
at d2a4fb6) with two changes to how cached views are reused and one to what a click
redraws. The owner chose
this on 21 Nov 2026 (shared/OWNER-DECISIONS.md, F146): patch GPUI until
upstream has an equivalent fix.

It is wired in by `[patch.crates-io]` in `native-v2/Cargo.toml`. To go back
to the published crate, delete that table and this folder. Cargo.lock then
gets the registry source and checksum back for `gpui-pre`.

Trimmed from the published crate: `examples/`, `benches/`, `tests/` and their
`[[example]]`, `[[bench]]` and `[[test]]` tables in `Cargo.toml`. Only gpui's
own unit tests used them, and they don't run here: `vendor/` is excluded from
the workspace. Everything else is byte for byte the published crate, except
the lines marked `crundi patch (a)`, `crundi patch (b)` or `crundi patch (c)` in:

- `src/view.rs`: `prepaint_view`, `paint_view`, `ViewElementState`,
  `view_state_type`, `view_state`, `shift_prepaint`, `shift_paint` and
  `inside`
- `src/window.rs`: the `Window` fields `unsettled`, `settle_seq`,
  `settle_task` and `frame_seq`; `PrepaintStateIndex::shifted`,
  `PaintIndex::shifted`, `reuse_prepaint_moved` (`reuse_prepaint` calls it),
  `offset_reusable`, `reuse_prepaint_at`, `note_unsettled` and
  `reuse_paint_at` (`reuse_paint` calls it); `frame_seq` counts up where
  the frames are swapped in `draw`
- `src/text_system/line_layout.rs`: `LineLayoutIndex::shifted`
- `src/scene.rs`: `can_offset` and `replay_offset`
- `src/elements/div.rs` (c): `paint_mouse_listeners` (the pending press,
  its release, the active style), `clear_active_tooltip_if_not_hoverable`,
  `register_tooltip_mouse_handlers` and `show_tooltip`

`crates/shell/tests/gpui_patch.rs` checks both behaviours in `cargo test`
on every OS: a clean cached child isn't rendered when its parent is, a moved
one is replayed and then settles with one render, and a resized one
renders.

Both changes are platform-independent (no `cfg`). They touch only the frame
building, not any OS's window or renderer.

## (a) A cached view that renders again keeps its clean cached children

**Upstream.** When a cached view (`Entity::cached`, `AnyView::cached`) is
dirty and renders again, `prepaint_view` sets `window.refreshing = true` for
its whole subtree. `refreshing` is the only flag a cached view checks besides
its own key and `dirty_views`. So every cached view inside one that changed
renders again, even when nothing it shows changed. Caching worked at one level
only. A layout view (the browser drawer's layout, a settings page) could not
be cached without its cached leaves (bars, sections) redrawing with it on
every frame of any of them.

**The change.** That one assignment is gone: rendering a view again leaves
`refreshing` as it was. A child whose state changed is still in
`dirty_views` (a notify marks the view and every view above it), so it still
renders. A clean child is replayed from its last frame, exactly as a cached
view under an uncached parent always was. A real window refresh
(`Window::refresh`, `App::refresh_windows`, a forced render after a GPU
recovery) still sets `refreshing`, and everything renders.

**What makes it sound.** A cached view records where its prepaint and
paint landed in the frame: its ranges in each of the frame's lists
(hitboxes, dispatch nodes, element states, line layouts, scene operations,
listeners and so on). It replays them from the previous frame. Upstream,
those records were always current, because nothing cached inside a replayed
view was ever replayed on its own. With (a) that changes:
- **Nested ranges follow their parent.** When a cached view's ranges are
  replayed, the cached views drawn inside them aren't visited. Their data is
  copied along with the parent's, to new positions. So
  `reuse_prepaint_moved` and `reuse_paint_at` shift each nested cached
  view's recorded ranges by the parent's shift, list by list
  (`PrepaintStateIndex::shifted`, `PaintIndex::shifted`). The nested views
  are found in the replayed range of accessed element states. Without the
  shift, the next time the parent renders, a child would replay ranges of an
  older frame: a panic in `reuse_layouts` ("range start index out of range")
  or wrong drawing.
- **Only what was painted in the previous frame is replayed.**
  `ViewElementState::painted_in` is the frame (`Window::frame_seq`) the view
  was last painted in, set by its own paint or by a shifted replay. A view
  whose last frame prepainted it without painting it renders rather than
  replay half-stale ranges.

The child's cache key (bounds, content mask, text style, opacity) still has
to match. A child the parent resized renders again; one it moved is
replayed moved, under (b).

## (b) A cached view that only moved is replayed at an offset (F130)

**Upstream.** A cached view's key includes its bounds' origin. Inside
anything that scrolls, every cached view moves on each scroll frame, misses
its cache and renders again. A view scrolled by the wheel cost as much as a
view that changed.

**The change.** In `prepaint_view`, a view whose key differs only in its
origin is replayed from its last frame, moved by the difference, when all of
these hold:
- its size, content mask, text style and opacity are as before, and it is
  neither dirty nor refreshing;
- the move is a whole number of device pixels on both axes, so glyph sprites
  keep their subpixel variant and land on the same pixel grid (a fractional
  move renders again);
- the view lies wholly inside its content mask before and after the move.
  The scene drops primitives outside the mask when they are inserted, so a
  view that was partly clipped may lack what is now in view: it renders
  again;
- its last frame has no deferred draws (an anchored popover is placed in
  window coordinates) and no sprite drawn through a transformation (its
  matrix is in window coordinates): `Window::offset_reusable`.

What is moved:
- **Prepaint:** the hitboxes, whose clips are moved and then intersected with
  the current content mask (`reuse_prepaint_at`).
- **Paint:** every scene primitive's bounds, content mask (moved, then
  intersected with the current mask), path vertices, a shadow's element
  bounds and layer bounds (`Scene::replay_offset`), and the test-support
  debug bounds.

Dispatch nodes, tab stops, cursor requests, tooltips and element states have
no positions and are replayed as they are. The cached views nested in a
moved one have their recorded bounds moved too.

**Settling.** Listeners an element registered while painting may have
captured window positions: a text input's bounds, say, for mapping a click
to a character. Those closures are replayed unchanged. So each moved view is
recorded (`Window::unsettled`), and 120 ms after the last move the window
notifies those views once (`note_unsettled`). They then render for real, and
what they captured is current again. While the wheel keeps moving them, the
timer keeps restarting, so a scroll costs one real render of each moved view
when it stops, not one per frame. A click landing on a moved view before
that render would use the old positions; within 120 ms of a scroll, that
is not something a person does.

## (c) A click and a tooltip redraw their own view, not the window (F105)

**Upstream.** `div.rs` answers a press on any element with a click
listener, its release, the active (pressed) style, and a tooltip shown or
cleared by the pointer with `window.refresh()`: every view in the window
renders again. With the panes, the top bar and the overlays cached, one
press-drag-release on a pane's head rendered every Shell and overlay view
one to two times (shared/uir-round3/shell.md).

**The change.** The press and its release change nothing that is drawn
(`pending_mouse_down` is read by the listeners and the tooltip checks, not
by a render), so they redraw nothing. The active style, a tooltip shown,
and a tooltip cleared by a press or the wheel notify the view that drew the
element (`window.current_view()` at paint, as upstream's own hover and
tooltip-visibility code already does): that view renders, its ancestors
with it, and their clean cached children are replayed. A tooltip hidden by
its timer or by the window's prepaint check still refreshes the window, as
upstream (they are rare and outside any scroll or drag). A GPUI drag
(`on_drag`, `cx.active_drag`) still redraws the window on every move,
as upstream.

## The upstream fix to watch for

Zed's gpui, `crates/gpui/src/view.rs`, `prepaint_view`:
- for (a), a cached view rendering again without
  `mem::replace(&mut window.refreshing, true)`, or nested view caching by
  another means;
- for (b), a cache key that tolerates an origin change: replay at an offset,
  or element-relative coordinates in the scene and hitboxes.

For (c), a click's or a tooltip's state change notifying its view rather
than `window.refresh()`.

When a `gpui-pre` release (the snapshot gpui-component pins) has all three,
delete the patch: the cached views here are written for the published
semantics plus these two. When it has only one, keep the other here, and
re-measure with `tools/measure-pc.mjs` on the owner's display either way.
