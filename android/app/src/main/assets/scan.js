// The scanning screen: hold the camera, look at what it sees a few times a
// second, and say so when one of those looks is a pairing picture.
//
// decodeImage is the server's own reader (src/crundi-code.js, copied here at
// build time). It finds the Crundi code and returns the link in it, or null;
// it does not guess, so a null just means "not yet".
import { decodeImage } from './crundi-code.js';

const video = document.getElementById('v');
const say = document.getElementById('say');
const frame = document.getElementById('frame');
const native = window.CrundiScan || { found: (l) => console.log('found', l), cancel: () => {} };
document.getElementById('cancel').onclick = () => native.cancel();

const canvas = document.createElement('canvas');
const ctx = canvas.getContext('2d', { willReadFrequently: true });
// The fallback picture is an ordinary QR code of the same link. Where the
// system offers a reader for those, use it; where it does not, the Crundi
// code is the only thing this screen reads.
let qr = null;
try { if ('BarcodeDetector' in window) qr = new window.BarcodeDetector({ formats: ['qr_code'] }); } catch (e) { qr = null; }

let done = false;
async function look() {
  if (done) return;
  const w = video.videoWidth, h = video.videoHeight;
  if (w && h) {
    // 640 on the long side is plenty and keeps each look to a few milliseconds.
    const scale = Math.min(1, 640 / Math.max(w, h));
    canvas.width = Math.round(w * scale); canvas.height = Math.round(h * scale);
    ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
    let link = null;
    try { const got = decodeImage(ctx.getImageData(0, 0, canvas.width, canvas.height)); if (got) link = got.link; } catch (e) { /* a bad frame; the next one is on its way */ }
    if (!link && qr) {
      try { const codes = await qr.detect(canvas); const hit = codes.find((c) => /^https?:\/\/[^\s]+\/pair\/?#[a-f0-9]{24}$/.test(c.rawValue || '')); if (hit) link = hit.rawValue; } catch (e) { /* not available after all */ }
    }
    if (link) {
      done = true;
      frame.className = 'hit';
      say.textContent = 'Got it';
      native.found(link);
      return;
    }
  }
  setTimeout(look, 140);
}

navigator.mediaDevices.getUserMedia({ audio: false, video: { facingMode: { ideal: 'environment' }, width: { ideal: 1280 }, height: { ideal: 720 } } })
  .then((stream) => { video.srcObject = stream; return video.play(); })
  .then(look)
  .catch(() => { say.textContent = 'The camera could not be opened. Go back and paste the pairing link instead.'; });
