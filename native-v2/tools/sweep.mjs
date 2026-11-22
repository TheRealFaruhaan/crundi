// The wheel sweep both harnesses use (measure-pc.mjs, measure-floor.mjs):
// one wheel event every 7 ms over a grid of points across a 1920x1040
// window (every pane is under some), 20 up then 20 down at each point.
import { sleep } from '../test/lib.mjs';

export async function sweep(d, ms) {
  const xs = [320, 620, 920, 1220, 1520, 1800], ys = [260, 520, 800];
  const end = Date.now() + ms;
  let i = 0;
  while (Date.now() < end) {
    const x = xs[Math.floor(i / 40) % xs.length], y = ys[Math.floor(i / 240) % ys.length];
    await d.send({ cmd: 'mouse', kind: 'wheel', x, y, dy: (i % 40) < 20 ? -60 : 60 });
    i++;
    await sleep(7);
  }
  return i;
}
