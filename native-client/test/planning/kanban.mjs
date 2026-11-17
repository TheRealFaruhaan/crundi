// Planning 4.1 Kanban and 4.2 the card dialog (run by test/planning.mjs).

let T;
// (GPUI: the driver has no `areas` map yet, shared/requests.md; the planning part also answers as `planningArea`.)
const K = (p) => (p.areas?.planning ?? p.planningArea)?.kanban;
const A = (p) => (p.areas?.planning ?? p.planningArea)?.ask;
// (GPUI: the driver in this tree has no window-wide "dialog" / "menu" props, its dialog-answer
// only closes, and lib.rs puts the planning part's props under planningArea, so pointerModal,
// dialog and menu are read there too, and the shared prompt / menu are answered through the
// planning part's own dialog-answer / menu-pick: planning-asks.md. The checks are the same.)
const PA = (p) => p.areas?.planning ?? p.planningArea ?? {};
const modal = (p) => !!(p.pointerModal || PA(p).pointerModal);
const menu = (p) => PA(p).menu ?? p.menu;
const dialog = (p) => PA(p).dialog ?? p.dialog;
const answerDialog = (ok, value = '') => T.act('dialog-answer', ok ? 'true' : 'false', value);
const pick = (id) => T.act('menu-pick', id);
const col = (p, st) => K(p)?.columns?.find((c) => c.status === st && !c.other);
const cardBy = (p, title) => K(p)?.columns?.flatMap((c) => c.cards.map((x) => ({ ...x, status: c.status }))).find((x) => x.title === title);
const kpost = (body) => T.api.http('POST', '/api/kanban', { project: 'demo', ...body });

export async function run(T_, t) {
  T = T_;
  // ── 4.1 Kanban: the empty board ──
  await T.ui('tab', 'kanban');
  await T.until('the board loads with four columns', (p) => K(p)?.state === 'ok' && K(p).columns.length === 4
    && K(p).columns.map((c) => c.label).join() === 'Backlog,To Do,In Progress,Done');
  await T.snap('kanban-empty');

  // The toolbar's + Add task: the card dialog with its Column choice.
  await T.act('kanban-act', 'add-task');
  await T.until('Add task asks for a title and a column', (p) => A(p)?.title === 'Add task' && A(p).label === 'Task title'
    && A(p).choices.join() === 'backlog,todo,in_progress,done' && A(p).ok === 'Add task' && modal(p));
  await T.snap('kanban-add-task');
  await T.act('ask-answer', 'true', 'Write the plan', 'in_progress');
  await T.until('the task lands in In Progress', (p) => cardBy(p, 'Write the plan')?.status === 'in_progress');
  await T.act('kanban-act', 'add-task');
  await T.until('the column picked last is remembered', (p) => A(p)?.choice === 'in_progress');
  await T.act('ask-answer', 'false');
  await T.until('Cancel closes it', (p) => !A(p) && !modal(p));

  // "+ Add task" at a column's end: the blank card; Enter adds and stays open.
  await T.act('kanban-act', 'add-task-inline', '', 'backlog');
  await T.until('the blank card opens in Backlog', (p) => col(p, 'backlog')?.adding);
  await T.act('kanban-type-task', 'First card');
  await T.act('kanban-commit-task', 'First card', 'true');
  await T.until('Enter adds it, the blank card stays', (p) => cardBy(p, 'First card')?.status === 'backlog' && col(p, 'backlog').adding);
  await T.act('kanban-commit-task', 'Second card', 'false');
  await T.until('leaving it adds and closes', (p) => cardBy(p, 'Second card') && !col(p, 'backlog').adding);

  // Subtasks: the row opens on a card, Enter adds, the progress counts.
  const first = cardBy(await T.props(), 'First card');
  t.check('a card without subtasks is compact', first.compact, first);
  await T.act('kanban-act', 'add-subtask', first.id);
  await T.until('the new subtask row opens', (p) => cardBy(p, 'First card').adding && !cardBy(p, 'First card').compact);
  await T.act('kanban-commit-todo', 'Step one', 'true');
  await T.until('the subtask is added', (p) => cardBy(p, 'First card').total === 1);
  await T.act('kanban-commit-todo', 'Step two', 'false');
  await T.until('a second, and the row closes', (p) => cardBy(p, 'First card').total === 2 && !cardBy(p, 'First card').adding);
  let c1 = cardBy(await T.props(), 'First card');
  await T.act('kanban-toggle', c1.id, c1.todos[0].id, 'true');
  await T.until('ticking counts it done', (p) => cardBy(p, 'First card').done === 1);

  // The card's menu: its actions row.
  await T.act('kanban-act', 'card-menu', c1.id);
  await T.until('the menu opens the actions', (p) => cardBy(p, 'First card').menuOpen);
  await T.snap('kanban-board');

  // The subtask's "…": the shared menu (Attach media, Rename, Delete).
  await T.act('kanban-todo-menu', c1.id, c1.todos[1].id, '700', '300');
  await T.until('the subtask menu', (p) => menu(p)?.items?.map((i) => i.id).join() === 'media,rename,delete' && menu(p).items[2].danger);
  await T.snap('kanban-subtask-menu');
  await pick('rename');
  await T.until('Rename asks', (p) => dialog(p)?.title === 'Edit subtask' && dialog(p).value === 'Step two');
  await answerDialog(true, 'Step 2');
  await T.until('the subtask is renamed', (p) => cardBy(p, 'First card').todos[1].text === 'Step 2');
  await T.act('kanban-todo-menu', c1.id, c1.todos[1].id, '700', '300');
  await T.until('the menu again', (p) => !!menu(p));
  await pick('delete');
  await T.until('the subtask is deleted', (p) => cardBy(p, 'First card').total === 1);

  // 4.2 The card dialog: Edit (title and column, Next, then the description).
  await T.act('kanban-act', 'edit-task', c1.id);
  await T.until('Edit task: title and column', (p) => A(p)?.title === 'Edit task' && A(p).value === 'First card' && A(p).choice === 'backlog' && A(p).ok === 'Next');
  await T.act('ask-answer', 'true', 'First card, edited', 'todo');
  await T.until('then the description', (p) => A(p)?.label === 'Description' && A(p).multiline && A(p).ok === 'Save');
  await T.snap('kanban-edit-description');
  await T.act('ask-answer', 'true', 'What it is about');
  await T.until('saved and moved to To Do', (p) => { const c = cardBy(p, 'First card, edited'); return c?.status === 'todo' && c.desc === 'What it is about'; });

  // Drag to move: a real mouse drag from Backlog's card to the Done column.
  await T.act('kanban-act', 'card-menu', c1.id);
  const second = cardBy(await T.props(), 'Second card');
  await T.act('kanban-measure');
  const boxes = await T.until('the boxes are measured', (p) => K(p).boxes.cards.some((b) => b.id === second.id) && K(p).boxes.cols.length === 4);
  const sb = K(boxes).boxes.cards.find((b) => b.id === second.id);
  const done = K(boxes).boxes.cols.find((b) => b.status === 'done');
  const sx = sb.x + sb.w / 2, sy = sb.y + 10;
  await T.mouse('move', sx, sy);
  await T.mouse('down', sx, sy);
  for (let i = 1; i <= 8; i++) await T.mouse('move', sx + (done.x + done.w / 2 - sx) * i / 8, sy + (done.y + 40 - sy) * i / 8);
  await T.until('the drop line is in Done', (p) => K(p).drag?.id === second.id && K(p).drag.status === 'done');
  await T.snap('kanban-drag');
  await T.mouse('up', done.x + done.w / 2, done.y + 40);
  await T.until('the card moved to Done', (p) => cardBy(p, 'Second card')?.status === 'done' && !K(p).drag);

  // Live: a change from elsewhere shows without a reload.
  await kpost({ action: 'addTask', title: 'From the server', status: 'todo' });
  await T.until('a server-side change arrives live', (p) => cardBy(p, 'From the server')?.status === 'todo');

  // Delete, Trash, Restore; History.
  const srv = cardBy(await T.props(), 'From the server');
  await T.act('kanban-act', 'del-task', srv.id);
  await T.until('deleted, the Trash counts it', (p) => !cardBy(p, 'From the server') && K(p).trash === 1);
  await T.act('kanban-act', 'view-trash');
  await T.until('the Trash lists it and the deleted subtask', (p) => K(p).view === 'trash' && K(p).deletedTasks[0]?.text === 'From the server' && K(p).deletedTodos.some((x) => x.text === 'Step 2'));
  await T.snap('kanban-trash');
  await T.act('kanban-act', 'restore-task', srv.id);
  await T.until('Restore brings it back', (p) => K(p).trash === 0);
  await T.act('kanban-act', 'view-history');
  await T.until('History lists the changes, newest first', (p) => K(p).view === 'history' && K(p).history.length >= 5 && K(p).history[0].ts.includes('/'));
  await T.snap('kanban-history');
  await T.act('kanban-act', 'view-board');
  await T.until('back to the board', (p) => K(p).view === 'board' && cardBy(p, 'From the server'));

  // The panel in a workbench pane, narrow: the columns stack.
  await T.ui('add-pane', 'kanban');
  await T.until('a Kanban pane', (p) => p.panes.some((x) => x.key === 'panel:kanban'));
  await T.ui('tab', 'workbench');
  await T.until('the workbench shows', (p) => p.currentTab === 'workbench');
  await T.snap('kanban-panel');
  await T.ui('tab', 'kanban');
}
