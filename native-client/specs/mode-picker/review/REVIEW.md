# 2.6 Permission-mode picker: design review (round 1)

Run on a fresh `tools/test-server.sh 4814` (chats: Fruit list (seeded), Skip chat
(skipPermissions), Plain chat, Gone), the web UI at 1400×860 next to the native client
(same window, real X input through xdotool and the testctl driver). Each image is web
above (or left of) native.

| State | Image | Result |
|---|---|---|
| Buttons at rest (Ask, Bypass all red, Ask, Ask) | rest-buttons-* | same |
| Buttons per mode, narrow (Accept edits, Don't ask, Plan sky border, Auto); set live by native and by the API | buttons-* | same; live from another client follows |
| Picker on a skip chat (6 modes, Bypass all first, selected) | open-bypass-* | same parts, order, wording |
| Option hover (Auto) | hover-option-* | same |
| Picker on a plain chat (5 modes) | open-default-* | same |
| Keyboard: focus on the selected at open, Tab, Shift+Tab to ×, wrap | keyboard-focus-native.png | works, stays in the dialog (named difference) |
| Narrow "…" tray with PLANNING before CHAT | tray-* | same |
| Full-screen pane: header PLANNING, Plan mode button, hover keeps sky border, tooltip | full-head-*, full-meta-* | same |
| Picker over a full-screen pane, Esc | full-open-native-window.png | closes; pane stays full screen |
| Running chat: open, choose Auto; the turn carries on | running-changed-* | same (Stop/Queue/working… stay) |
| Exited chat: refused change | refused-native-window.png | button back to Auto, toast "Could not switch permission mode: No session "…"" |
| Window 420 high | short-window.png | dialog sp-16 from the edges, scrolls with its header |
| Backdrop: click on a pane control, drag a gutter, wheel | (driver) | blocked; a click closes the picker |

## Differences

1. **Hover goes through the backdrop (blocking).** With the picker open, moving the
   mouse over any pane under the scrim shows that pane's split controls (the two
   split icons in its header), dimmed under the scrim. Web: the backdrop takes the
   hover; no pane reacts (`hover-under-scrim-header.png`: web above, native below,
   mouse over the left pane in both). Clicks, drags and the wheel are blocked; only
   the hover (the `Pointer` global the panes read) leaks.
2. **Keyboard focus does not scroll into view in a short window (blocking).** At
   420 px high, Tab to Don't ask leaves it half under the dialog's bottom edge; Tab to
   × after scrolling down leaves × out of sight (`short-window-tab-focus-native.png`).
   Web: focusing an option or × scrolls the dialog to show it (measured: scrollTop
   0 → 10 for the last option, back for ×). The same holds at open, when the chat's
   mode is below the fold.
3. **No scroll indicator in a short window (nit).** The web dialog shows its scrollbar
   when it scrolls (`short-window.png`, right edge); the native one has none. Same
   kind as F49; the composer's thumb could be reused.

Not differences: option heights (lh-chat line height, named in 2.5), the focus ring
on open (named in PLAN.md), the 404 toast (code path + unit test; not stubbed live).

F49: verified, the composer shows its thumb past composer-max
(`specs/composer/review/F49-scroll-thumb-native.png`).
