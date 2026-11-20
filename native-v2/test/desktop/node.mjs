// ─── The homepool node: off until shared ───
// (native-gpui/test/desktop.mjs's section.) Nothing runs for it until the
// owner shares this PC from the Homepool tab.

export async function run(T, t) {
  const r = await T.act('homepool-status');
  t.check('the node is off, with this PC\'s id', r.ok && r.status.state === 'off' && /^pc-/.test(r.status.deviceId), r);
  t.check('…not set up, no streams, nothing sent', r.status.configured === false && r.status.enabled === false && r.status.streams === 0 && r.status.bytesUp === 0 && r.status.bytesDown === 0, r.status);
}
