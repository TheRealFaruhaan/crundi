# gpui-pre 0.3.12, patched for native-v2

This is the published `gpui-pre` 0.3.12 (crates.io; a snapshot of Zed's gpui
at d2a4fb6) with two changes to how cached views are reused, one to what a click
redraws and one to how a replayed view enters the scene (d). The owner chose
this on 21 Nov 2026 (shared/OWNER-DECISIONS.md, F146): patch GPUI until
upstream has an equivalent fix.

It is wired in by `[patch.crates-io]` in `native-v2/Cargo.toml`. To go back
to the published crate, delete that table and this folder. Cargo.lock then
gets the registry source and checksum back for `gpui-pre`.

Trimmed from the published crate: `examples/`, `benches/`, `tests/` and their
`[[example]]`, `[[bench]]` and `[[test]]` tables in `Cargo.toml`. Only gpui's
own unit tests used them, and they don't run here: `vendor/` is excluded from
the workspace. Everything else is byte for byte the published crate, except
the lines marked `crundi patch (a)` to `crundi patch (d)` (and the
`crundi_phases` diagnostic) in:

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
- `src/scene.rs`: `can_offset`, `extent` and `replay_offset`; (d)
  `replay_shifted` (`replay` and `replay_offset` call it) and
  `Primitive::order`
- `src/bounds_tree.rs` (d): `BoundsTree::insert_span`
- `src/window.rs` (d): `crundi_scene_batches` (tests only); the
  `crundi_phases` module and its calls in `draw_roots`, `draw` and `present`
- `src/key_dispatch.rs`: `DispatchTree::focus_in`
- (b) in `src/view.rs` also `reveals_nothing`, `outside` and
  `ViewElementState::skipped`; in `src/window.rs` `painted_extent`,
  `deferred_in` and `focus_drawn_in`
- `src/elements/div.rs` (c): `paint_mouse_listeners` (the pending press,
  its release, the active style), `clear_active_tooltip_if_not_hoverable`,
  `register_tooltip_mouse_handlers` and `show_tooltip`

`crates/shell/tests/gpui_patch.rs` checks both behaviours in `cargo test`
on every OS: a clean cached child isn't rendered when its parent is, a moved
one is replayed and then settles with one render, and a resized one
renders; one moving out across its clip's edge is replayed, one moving back
in renders, and one wholly outside isn't rendered, even when it changes,
until it is in view again.

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
- the move brings into view nothing its content mask cut off before
  (F241). The scene keeps only what lies inside the mask (a primitive wholly
  outside is dropped when it is inserted, an element may skip what it can't
  see), so a view coming into view across its clip's edge renders again. A
  view wholly inside its mask before and after qualifies at once. Otherwise
  its extent (its bounds and what its last paint drew, unclipped:
  `Window::painted_extent`, `Scene::extent`) is taken back by the move, and
  the part of it now in view must have been in view before
  (`reveals_nothing`). A pane crossing the grid's edge on its way out, or
  moving further out of it, is replayed; one coming in renders;
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

A replayed primitive's clip is moved, then intersected with the current
mask, which is right where it is needed: inside the part now in view, which
was in view before.

Dispatch nodes, tab stops, cursor requests, tooltips and element states have
no positions and are replayed as they are. The cached views nested in a
moved one have their recorded bounds moved too.

**Wholly outside its clip (F241).** A cached view none of whose extent (its
bounds and, at the last paint, what it drew unclipped) is inside its content
mask is neither rendered nor painted, whether it is dirty or not
(`ViewElementState::skipped` keeps that extent, relative to its origin).
Its prepaint and paint ranges are empty, its entities aren't accessed, so a
change in them doesn't wake the window: a terminal printing in a pane out of
the grid's view draws nothing. It renders as soon as any of its extent is in
view again, or the window refreshes. Not skipped: a view with a deferred draw
last frame (an anchored popover may be anywhere), one holding the focus (its
dispatch nodes must stay in the tree for keys and blur;
`Window::focus_drawn_in`, `DispatchTree::focus_in`), one whose size changed,
and any view during a window refresh (a focus change refreshes the window,
so a view given the focus while hidden renders).

**What the checks cost.** Both tests look at the view's bounds first (a few
comparisons) and scan its scene operations for the extent only when the
bounds alone pass: a view whose bounds are outside its clip, or whose move
reveals nothing of its bounds. A first version scanned every cached view's
operations on every frame and cost planning+stream+scroll about 10% of its
frames under 7 ms on the PC (perf/pc-2026-11-22-architect-f241.md).

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

## (d) A replayed view keeps its draw orders (F245)

**Upstream.** `Scene::replay` copies a cached view's last scene operations
into the new frame through `insert_primitive`, one by one, as if they were
painted fresh: each primitive's clipped bounds go into the frame's
`BoundsTree` (an R-tree) to find its draw order (one above the highest order
it overlaps), and leave a leaf there. With the panes, heads and shell views
cached, most of a frame is replay: every primitive of every pane (each glyph
a sprite) went through the tree on every frame, and the tree held a leaf for
each of them.

**The change.** `replay_shifted` replays a range without the tree per
primitive. One pass reads the range's lowest and highest order and its
extent (the union of its primitives' clipped bounds and its layers' bounds,
moved and cut to the current mask when it is replayed moved). One
`insert_span` then finds the highest order the extent overlaps, and puts one
leaf over the extent holding the range's highest new order. Each primitive
keeps the order it had, shifted by the same amount: `order - lowest + base`,
where `base` is one above what the extent overlaps. Its path id is given
anew; a moved replay moves and clips it as `replay_offset` did.

**What makes it sound.** Draw order matters only between primitives that
overlap. Inside the range, every pair keeps its relative order, so what
overlapped draws as it did (and batches as it did). Everything drawn before
the range that overlaps any of it overlaps the extent, so it is below
`base`. Everything drawn after over any of it overlaps the extent's leaf,
so the tree puts it above the range's highest. Orders may be higher than
upstream would give (the extent is coarser than each primitive), which
changes no overlap's outcome. A range replayed inside an open layer
(`push_layer`) takes the plain path, as upstream: there, every primitive
takes the layer's order anyway, and the tree isn't touched.
`crates/shell/tests/gpui_patch.rs` (`replayed_views_draw_in_the_same_order`)
checks that a frame replaying a cached view, in place and moved, twice in a
row, batches exactly as the frame that rendered it.

## The `crundi_phases` diagnostic

With `CRUNDI_FRAME_PHASES=1`, each window prints on stderr, every 2 s that
drew, the mean time of the frames' phases (request layout; layout and
prepaint; paint; finishing the frame, where the scene is sorted; present)
and the scene's mean primitive and operation counts. Unset, each phase reads
one cached flag. It changes nothing drawn.

## The upstream fix to watch for

Zed's gpui, `crates/gpui/src/view.rs`, `prepaint_view`:
- for (a), a cached view rendering again without
  `mem::replace(&mut window.refreshing, true)`, or nested view caching by
  another means;
- for (b), a cache key that tolerates an origin change: replay at an offset,
  or element-relative coordinates in the scene and hitboxes.

For (c), a click's or a tooltip's state change notifying its view rather
than `window.refresh()`.

For (d), a replay that keeps draw orders (or a scene that refers to the
previous frame's primitives instead of copying them).

When a `gpui-pre` release (the snapshot gpui-component pins) has all four,
delete the patch: the cached views here are written for the published
semantics plus these two. When it has only one, keep the other here, and
re-measure with `tools/measure-pc.mjs` on the owner's display either way.
