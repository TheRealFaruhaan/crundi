# Permission card (F107), round 2

Same entries injected on both sides (web: page WebSocket `ui-event`; native: testctl chat-event):
pending Bash with description, decisionReason (ANSI), input; denied Write; unanswered
"Browser open" with title; always allowed by Cru Edit; allowed Read; then a pending with
`suggestions`, an escalated one, and one escalated to the Manager.

Matches (pending.png, closed.png): yellow card, "Claude wants to use Bash", description,
reason with ANSI stripped, the input as pretty JSON in the mono box, the bold name + tag for
each closed status (red denied / never answered, green allowed / always allowed by Cru), the
title under the never-answered card.

Differences (always-escalated.png, web over native):
1. "Allow always" — web shows it only when the request has `suggestions` and no
   `suppressAlwaysAllow`; native shows it on every pending card (pending.png: web Allow, Deny;
   native Allow, Allow always, Deny).
2. Escalated — web: `escalated` shows "Sent to the project owner. You will see the answer
   here." and one button "Carry on without this" (POST /api/collab/cancel {id: approvalId});
   `escalated` + `routedTo: 'manager'` shows "Sent to the Manager of this factory, who
   decides. You will see the answer here." and no buttons. Native: Allow / Allow always / Deny
   on both.
