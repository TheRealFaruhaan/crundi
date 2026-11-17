# Panels (widgets): the tab, the widget pane, the inline card, docks, chips

Reference: `app/vendor/crundi-widgets.js` (all of it), `src/webapp-html.js`
(`widgetsOn`, `loadWidgetsTab`, `widgetToolExtra`, the `widget` cells and
`wpick` chooser), server `src/widget-api.js`, `src/widget-doc.js`,
`src/widget-runtime/runtime.js`. Native: `ui/ops/panels.slint`,
`src/ops/panels.rs`, `test/ops/panels.mjs`.

## Parts

- **Panels tab** (`panels`): list column (panels-list-w, bg-secondary, rows:
  panel icon, title, badge `global` / `approve` / `error` or the line
  `closed`, `docked`, `workbench`, `panels`, `inline`, `top`), open first then
  newest. Main: bar (title; slot select when open; Pin/Pinned; Make global /
  Global (primary) / badge "global · from X"; Reload; Checks; History;
  Close / Reopen (primary); Delete (danger, confirm)), then the stage: the
  panel, "This panel is closed.", Checks (shots of the last widget_render, or
  "No check has been run yet…"), History (vN, date, `current` or Restore).
- **Empty**: "No panels yet" + hint; no project and no global panels: "No
  global panels yet." + hint. Loading; an error with Retry.
- **Widget pane** (`widget`): key `widget:<id>` the panel filling the pane;
  `panel:widget` the chooser ("CHOOSE A PANEL TO OPEN HERE", rows with
  status; "already open" greyed; empty: "No panels in this project yet" +
  hint). Picking patches `{closed:false, slot:'cell'}`.
- **Frame** (every copy): approval banner (what it asks, Approve / Not now),
  error banner, the page in an off-screen CEF view (frames.rs).
- **Inline card** (`OpsWidgetCard`): head (icon, title, "Open in the Panels
  tab"), the panel at its content height (≤ panels-inline-max).
- **Dock** (`OpsWidgetDock`): per docked panel a head (title folds, approve,
  slot select, collapse, ×) over the panel at content height.
- **Chips** (`OpsPanelChips`): pill (icon, label, value, approve) or the live
  chip face; click: popover (head: title, slot select, ×; the panel).

## Endpoints

`GET /api/widgets?project=`, `GET /api/widgets/doc` (`&chip=1`),
`GET /api/widgets/data` (`&source=`), `POST /api/widgets/user` (patch: slot,
closed, pinned, collapsed), `POST /api/widgets/scope`, `/grant`, `/fault`,
`/call`, `/delete`, `/rollback`, `/watch` (every 25 s),
`GET /api/widgets/versions`, `/renders`, `/render-image`. Event `widget`
(`kind`: data, store, doc, render, list…).

## Bridge (Slint/CEF limit, PLAN.md known gap)

frames.rs has no script injection or message hook yet (requested). The page
gets its `init` baked in; data/store/doc changes reload it; its messages
(height, fault, call) come out through its title. A call's answer cannot
reach the page (its promise times out); the call itself runs.

## Checklist

- [x] tab: empty, loading, error, list, selected, closed, checks, history
- [x] bar: slot, pin, global/back/badge, reload, checks, history, close/reopen, delete
- [x] frame: loads, paints, input, height, faults, toast/openLink/calls
- [x] approval banner and approve
- [x] chooser and picked panel in a pane
- [x] inline card, dock, chips, popover (components; placement requested)
- [ ] widget panes from `cell_ids()` and their header (workspace, requested)
- [ ] live data without reloading the page (frames::eval, requested)
