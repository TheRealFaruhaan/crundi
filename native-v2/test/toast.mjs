#!/usr/bin/env node
// The shared toast (the web's toast(): text only, a coloured left edge,
// under the top bar on the right, 3.5 s): one line and two lines, each kind.
// (On the Schedule tab: the workbench's launcher animates UI Mode.)
import * as t from './lib.mjs';
const { sleep } = t;
const T = await t.begin('toast');
await T.ui('tab', 'schedule'); await sleep(800);
await T.ui('toast', 'success', 'Idle panes close after 30 minutes');
await sleep(400); await T.snap('widgets', 'toast-one-line');
t.check('the toast is on show', (await T.state()).toast === 'Idle panes close after 30 minutes');
await T.ui('toast', 'error', 'The server refused the upload: the file is larger than the limit this server allows for attachments in a chat.');
await sleep(400); await T.snap('widgets', 'toast-two-lines');
await T.ui('toast', 'info', 'Added to chat');
await sleep(400); await T.snap('widgets', 'toast-info');
await sleep(3600);
t.check('it goes after 3.5 s', (await T.state()).toast === '');
const m = T.frames.mark(); await sleep(1500);
t.check('idle after it: no frames', T.frames.since(m).frames === 0, T.frames.since(m));
await T.end();
