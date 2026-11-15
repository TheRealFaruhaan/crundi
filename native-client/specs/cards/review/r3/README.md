# 2.20 cards, round 3: F124-F126 checked live

A real run on a test server (git project, factory, Manager's finish through the MCP bridge), driven in the native client:

- `fr-refused.png`: a refused Keep (stubbed), the action error under the buttons.
- `fr-changes.png`: Request changes with a note, sent: card `changes`, the note cleared, the form closed, the old error gone (F124, F126).
- `fr-again.png`: the Manager finishes again: `ask`, no note form, no error (F126).
- `fr-merge-refused.png`: Merge into dev with an uncommitted edit in the dev checkout: the server's error once, above the buttons; no action error line (F125).
- `f124-kept-no-error.png`: another refused Keep, then the run kept from another client (API): the kept card has no stale error (F124).

The card states were also read from the driver's props at each step (actErr, error, noteOpen, note).
