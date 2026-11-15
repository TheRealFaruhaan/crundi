# 2.19 goal and plan: design review, round 1

Same server and method as agent-dock/review. Web over native, same region.
The web's PLAN pill follows the server's chat list, so injected plans do not
hold on the web: the plan panel and pill are compared with the spec's own
web shots (real plans) instead (`plan-open-proposed-spec-web-over-native`,
`plan-executing-head`, `plan-close-hover-rejected`, `pill-hover`).

All match: goal bar (spinner + GOAL, condition elided, running / N checks,
clear with red hover and tooltip), stopped line, verdict cards
(`goal-clear-hover`, `goal-stopped`); PLAN pill in the narrow tray before
CHAT (`tray-proposed`) and in the wide header, proposed / executing /
rejected colours, hover fill, toggle; plan panel header (PLAN, status
wording, Close + hover), markdown body; the tray above the panel
(`native-narrow-plan-open-tray.png`, named difference); `plan: null`
removes pill and panel. clear sent `/goal clear` through the send path
(`native-goal-clear-send.png`: the message and Claude's answer in the log).

Known gap accepted: plan headings are bold body text.
