# 2.21 review, round 1 (sent back)

Real side questions on both sides during a real turn (a permission prompt pending); errors
and the markdown answer stubbed (native testctl stub `aside`, web fetch override) with the
same text on both.

Matches: `/btw` caught in needs-input, never queued (drawer empty, chat state unchanged);
box cleared; log re-pinned from 339px up; card after the last entry; head, question,
spinner + "Asking…", answer, error box with the server's message, note; three cards at once
in order; × hover box + "Close" tooltip; × (real click) removes only that card.

Differences (web over native, aligned on the card's top):
1. pending.png: pending card — "Asking…" is pushed down and "Not added to the conversation."
   sits on the card's dashed bottom edge (web: "Asking…" 6px under the question, note 6px
   under it, 8px padding below). Cause: `al` (VerticalLayout in the aside card) has no
   height, so it fills the card (preferred + sp-16) and stretches its rows.
2. markdown.png: a list in the answer — native draws "• " inline at the text column and a
   wrapped item's next line goes back under the bullet; the spec's decision is "lists
   indented as in replies" (web: bullets hang outside, wrapped lines align with the item's
   text). Neither.
