//! "Hey Cru" training and matching (app/voice-wake/enroll.js, ported as it
//! is): five recordings of the owner give a profile: the keyword lines for
//! the model (the threshold it needs to hear all five, then two notches
//! looser), the decoys it may ignore, and the owner's phrase as MFCC
//! templates with the distance limits a burst must meet (DTW, whole and per
//! onset window). Pure: the model is given as a `Spot` (engine.rs).

use serde_json::{json, Value};

pub const RATE: usize = 16000;
pub const SAMPLES_NEEDED: usize = 5;
pub const SPELLINGS: [&str; 3] = ["▁HE Y ▁C RU", "▁HE Y ▁K RU", "▁HE Y ▁C RE W"];
pub const WAKE: &str = "HEY_CRU";
pub const DECOYS: &[(&str, &str)] = &[
    ("YOU", "▁HE Y ▁YOU"), ("YOUR", "▁HE Y ▁YOUR"), ("WHO", "▁HE Y ▁WHO"), ("HOO", "▁HE Y ▁HO O"), ("HUGH", "▁HE Y ▁ H UGH"),
    ("HUE", "▁HE Y ▁HU E"), ("TWO", "▁HE Y ▁TWO"), ("TO", "▁HE Y ▁TO"), ("DO", "▁HE Y ▁DO"), ("NEW", "▁HE Y ▁NEW"),
    ("TRUE", "▁HE Y ▁T RU E"), ("THROUGH", "▁HE Y ▁THROUGH"), ("SUE", "▁HE Y ▁SU E"), ("SHOE", "▁HE Y ▁S HO E"), ("RU", "▁HE Y ▁RU"),
    ("ROO", "▁HE Y ▁RO O"), ("DREW", "▁HE Y ▁DR E W"), ("GREW", "▁HE Y ▁G RE W"), ("SCREW", "▁HE Y ▁S C RE W"), ("CUE", "▁HE Y ▁C U E"),
    ("COOL", "▁HE Y ▁CO OL"), ("BLUE", "▁HE Y ▁B LU E"), ("GLUE", "▁HE Y ▁G LU E"), ("THERE", "▁HE Y ▁THERE"), ("WHAT", "▁HE Y ▁WHAT"),
    ("HOW", "▁HE Y ▁HOW"), ("LOOK", "▁HE Y ▁LOOK"), ("GOOGLE", "▁HE Y ▁GO O G LE"), ("OKAY_WHO", "▁OKAY ▁WHO"), ("OKAY_YOU", "▁OKAY ▁YOU"),
];
pub const THRESHOLDS: [f32; 7] = [0.35, 0.3, 0.25, 0.2, 0.15, 0.1, 0.06];
pub const KEYWORDS_SCORE: f32 = 1.5;
pub const CONFIRM_THRESHOLD: f32 = 0.04;
const LIMIT_MARGIN: f64 = 1.6;
const STRICT_MARGIN: f64 = 1.3;
const LENGTH_RANGE: (f64, f64) = (0.65, 1.5);
const MIN_VOICED: f64 = 0.3;
const MODEL_TRUSTED: i64 = 3;
const WINDOW: usize = 15;
const WINDOW_MARGIN: f64 = 1.6;
pub const VERIFY_SECONDS: f32 = 2.0;

/// The model, as the training needs it: what it hears in a clip (keyword names).
pub trait Spot {
    fn accept(&mut self, samples: &[f32]) -> Option<String>;
    fn reset(&mut self);
}

pub type Feats = Vec<Vec<f64>>;

fn fmt(x: f32) -> String { let s = format!("{x}"); s }

pub fn keyword_lines(threshold: f32, score: f32) -> String {
    SPELLINGS.iter().map(|sp| format!("{sp} :{} #{} @HEY_CRU", fmt(score), fmt(threshold))).collect::<Vec<_>>().join("\n")
}
pub fn decoy_lines(threshold: f32, score: f32, decoys: &[(&str, &str)]) -> String {
    decoys.iter().map(|(n, sp)| format!("{sp} :{} #{} @NOT_{n}", fmt(score), fmt(threshold))).collect::<Vec<_>>().join("\n")
}
pub fn competing_lines(threshold: f32, score: f32, decoys: &[(&str, &str)]) -> String {
    if decoys.is_empty() { keyword_lines(threshold, score) } else { format!("{}\n{}", keyword_lines(threshold, score), decoy_lines(threshold, score, decoys)) }
}

/// The decoys a profile keeps (all, for a profile from before).
pub fn decoys_of(profile: &Value) -> Vec<(&'static str, &'static str)> {
    match profile.get("decoys").and_then(|d| d.as_array()) {
        Some(a) => DECOYS.iter().copied().filter(|(n, _)| a.iter().any(|x| x.as_str() == Some(n))).collect(),
        None => DECOYS.to_vec(),
    }
}
fn pf(p: &Value, k: &str) -> f64 { p.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) }
pub fn listen_lines(profile: &Value) -> String {
    let d = decoys_of(profile);
    let kw = profile.get("keyword").and_then(|k| k.as_str()).unwrap_or("").to_string();
    if d.is_empty() { kw } else { format!("{kw}\n{}", decoy_lines(pf(profile, "keywordsThreshold") as f32, pf(profile, "keywordsScore") as f32, &d)) }
}
pub fn confirm_lines(profile: &Value) -> String { competing_lines(CONFIRM_THRESHOLD, pf(profile, "keywordsScore") as f32, &decoys_of(profile)) }

// ─── MFCC (26 mels, 12 cepstra, their deltas) ───

const FRAME: usize = 400;
const HOP: usize = 160;
const NFFT: usize = 512;
const MELS: usize = 26;
const CEPS: usize = 12;
const STATIC_WEIGHT: f64 = 0.6;
const DELTA_WEIGHT: f64 = 1.8;

struct Tables { mel: Vec<Vec<f64>>, hamming: Vec<f64>, dct: Vec<Vec<f64>> }

fn tables() -> &'static Tables {
    static T: std::sync::OnceLock<Tables> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let hamming = (0..FRAME).map(|i| 0.54 - 0.46 * (2.0 * std::f64::consts::PI * i as f64 / (FRAME - 1) as f64).cos()).collect();
        let mel = |f: f64| 2595.0 * (1.0 + f / 700.0).log10();
        let hz = |m: f64| 700.0 * (10f64.powf(m / 2595.0) - 1.0);
        let (lo, hi) = (mel(20.0), mel(7600.0));
        let bins: Vec<usize> = (0..MELS + 2).map(|i| ((NFFT + 1) as f64 * hz(lo + (hi - lo) * i as f64 / (MELS + 1) as f64) / RATE as f64).floor() as usize).collect();
        let mut melb = vec![];
        for m in 1..=MELS {
            let mut row = vec![0.0; NFFT / 2 + 1];
            for k in bins[m - 1]..bins[m] { row[k] = (k - bins[m - 1]) as f64 / (bins[m] - bins[m - 1]).max(1) as f64; }
            for k in bins[m]..bins[m + 1] { row[k] = (bins[m + 1] - k) as f64 / (bins[m + 1] - bins[m]).max(1) as f64; }
            melb.push(row);
        }
        let dct = (1..=CEPS).map(|c| (0..MELS).map(|m| (2.0 / MELS as f64).sqrt() * (std::f64::consts::PI * c as f64 * (m as f64 + 0.5) / MELS as f64).cos()).collect()).collect();
        Tables { mel: melb, hamming, dct }
    })
}

fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 { j ^= bit; bit >>= 1; }
        j ^= bit;
        if i < j { re.swap(i, j); im.swap(i, j); }
    }
    let mut len = 2;
    while len <= n {
        let a = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (a.cos(), a.sin());
        let mut i = 0;
        while i < n {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (ur, ui) = (re[i + k], im[i + k]);
                let (xr, xi) = (re[i + k + len / 2], im[i + k + len / 2]);
                let vr = xr * cr - xi * ci;
                let vi = xr * ci + xi * cr;
                re[i + k] = ur + vr; im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr; im[i + k + len / 2] = ui - vi;
                let nr = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = nr;
            }
            i += len;
        }
        len <<= 1;
    }
}

pub fn mfcc(samples: &[f32]) -> Feats {
    let t = tables();
    let mut mels: Vec<Vec<f64>> = vec![];
    let (mut re, mut im) = (vec![0.0; NFFT], vec![0.0; NFFT]);
    let mut loudest: f64 = 0.0;
    let mut start = 0;
    while start + FRAME <= samples.len() {
        re.iter_mut().for_each(|x| *x = 0.0); im.iter_mut().for_each(|x| *x = 0.0);
        let mut prev = if start > 0 { samples[start - 1] as f64 } else { 0.0 };
        for i in 0..FRAME { let x = samples[start + i] as f64; re[i] = (x - 0.97 * prev) * t.hamming[i]; prev = x; }
        fft(&mut re, &mut im);
        let mut e = vec![0.0; MELS];
        for m in 0..MELS {
            let mut s = 0.0;
            for k in 0..=NFFT / 2 { let r = t.mel[m][k]; if r != 0.0 { s += r * (re[k] * re[k] + im[k] * im[k]) / NFFT as f64; } }
            e[m] = s; if s > loudest { loudest = s; }
        }
        mels.push(e);
        start += HOP;
    }
    // Spectral subtraction: the quietest fifth is the noise.
    if mels.len() >= 5 {
        let mut energy: Vec<(f64, usize)> = mels.iter().enumerate().map(|(i, e)| (e.iter().sum(), i)).collect();
        energy.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let q: Vec<usize> = energy.iter().take((mels.len() / 5).max(1)).map(|x| x.1).collect();
        let mut noise = vec![0.0; MELS];
        for &i in &q { for m in 0..MELS { noise[m] += mels[i][m] / q.len() as f64; } }
        for e in mels.iter_mut() { for m in 0..MELS { e[m] = (e[m] - 1.5 * noise[m]).max(e[m] * 0.05); } }
    }
    let floor = (loudest * 1e-3).max(1e-10);
    let mut out: Vec<Vec<f64>> = mels.iter().map(|e| (0..CEPS).map(|j| (0..MELS).map(|m| t.dct[j][m] * e[m].max(floor).ln()).sum()).collect()).collect();
    if out.is_empty() { return out; }
    let mut mean = vec![0.0; CEPS];
    for f in &out { for j in 0..CEPS { mean[j] += f[j] / out.len() as f64; } }
    for f in out.iter_mut() { for j in 0..CEPS { f[j] -= mean[j]; } }
    (0..out.len()).map(|t| {
        let a = &out[t.saturating_sub(1)];
        let b = &out[(t + 1).min(out.len() - 1)];
        let mut v = vec![0.0; CEPS * 2];
        for j in 0..CEPS { v[j] = STATIC_WEIGHT * out[t][j]; v[CEPS + j] = DELTA_WEIGHT * (b[j] - a[j]) / 2.0; }
        v
    }).collect()
}

/// The echo and bright variants of a clip (more templates from each take).
pub fn variants(clip: &[f32]) -> [Vec<f32>; 2] {
    let peak = |a: &[f32]| a.iter().fold(0f32, |p, x| p.max(x.abs()));
    let p0 = { let p = peak(clip); if p == 0.0 { 1.0 } else { p } };
    let norm = |mut a: Vec<f32>| { let p = peak(&a); let k = p0 / if p == 0.0 { 1.0 } else { p }; a.iter_mut().for_each(|x| *x *= k); a };
    let mut far = vec![0f32; clip.len()];
    let mut y = 0f32;
    for i in 0..clip.len() { y += 0.45 * (clip[i] - y); far[i] = y; }
    let g = |i: isize| if i >= 0 { far[i as usize] } else { 0.0 };
    let echo: Vec<f32> = (0..clip.len()).map(|i| { let i = i as isize; g(i) + 0.35 * g(i - 480) + 0.2 * g(i - 1120) + 0.1 * g(i - 1900) }).collect();
    let bright: Vec<f32> = (0..clip.len()).map(|i| clip[i] - 0.6 * if i > 0 { clip[i - 1] } else { 0.0 }).collect();
    [norm(echo), norm(bright)]
}

/// The voice in a recording, with `pad_ms` around it (None: nothing clear).
pub fn trim(samples: &[f32], pad_ms: usize) -> Option<&[f32]> {
    let step = RATE / 100;
    let n = samples.len() / step;
    if n < 5 { return None; }
    let rms: Vec<f64> = (0..n).map(|i| ((0..step).map(|k| { let x = samples[i * step + k] as f64; x * x }).sum::<f64>() / step as f64).sqrt()).collect();
    let mut sorted = rms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (floor, peak) = (sorted[(n as f64 * 0.1) as usize], sorted[n - 1]);
    if peak < 0.01 || peak < floor * 4.0 { return None; }
    let thr = (peak * 0.12).max(floor * 2.5);
    let (mut a, mut b) = (0, n - 1);
    while a < n && rms[a] < thr { a += 1; }
    while b > a && rms[b] < thr { b -= 1; }
    let pad = (pad_ms as f64 / 10.0).round() as usize;
    let from = a.saturating_sub(pad) * step;
    let to = (b + 1 + pad).min(n) * step;
    Some(&samples[from..to])
}

fn dist(a: &[f64], b: &[f64]) -> f64 { a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt() }

/// Subsequence DTW: the template anywhere in the clip, per template frame.
pub fn distance(template: &Feats, clip: &Feats) -> f64 {
    let (m, n) = (template.len(), clip.len());
    if m == 0 || n == 0 { return f64::INFINITY; }
    let (mut pc, mut pl) = (vec![0.0f64; n + 1], vec![0.0f64; n + 1]);
    let (mut cc, mut cl) = (vec![0.0f64; n + 1], vec![0.0f64; n + 1]);
    for i in 1..=m {
        cc[0] = f64::INFINITY; cl[0] = 0.0;
        for j in 1..=n {
            let x = dist(&template[i - 1], &clip[j - 1]);
            let (mut best, mut len) = (pc[j - 1], pl[j - 1]);
            if cc[j - 1] < best { best = cc[j - 1]; len = cl[j - 1]; }
            if pc[j] < best { best = pc[j]; len = pl[j]; }
            cc[j] = best + x; cl[j] = len + 1.0;
        }
        std::mem::swap(&mut pc, &mut cc); std::mem::swap(&mut pl, &mut cl);
    }
    let mut best = f64::INFINITY;
    for j in 1..=n { if pl[j] > 0.0 { best = best.min(pc[j] / pl[j].max(m as f64)); } }
    best
}

/// Whole-to-whole DTW, per step.
pub fn full_distance(a: &Feats, b: &Feats) -> f64 {
    let (m, n) = (a.len(), b.len());
    if m == 0 || n == 0 { return f64::INFINITY; }
    let (mut pc, mut pl) = (vec![f64::INFINITY; n + 1], vec![0.0; n + 1]);
    let (mut cc, mut cl) = (vec![0.0; n + 1], vec![0.0; n + 1]);
    pc[0] = 0.0;
    for i in 1..=m {
        cc[0] = f64::INFINITY; cl[0] = 0.0;
        for j in 1..=n {
            let x = dist(&a[i - 1], &b[j - 1]);
            let (mut best, mut len) = (pc[j - 1], pl[j - 1]);
            if cc[j - 1] < best { best = cc[j - 1]; len = cl[j - 1]; }
            if pc[j] < best { best = pc[j]; len = pl[j]; }
            cc[j] = best + x; cl[j] = len + 1.0;
        }
        std::mem::swap(&mut pc, &mut cc); std::mem::swap(&mut pl, &mut cl);
    }
    if pl[n] > 0.0 { pc[n] / pl[n] } else { f64::INFINITY }
}

/// The worst stretch of WINDOW frames along the best path (an onset that differs).
pub fn window_distance(a: &Feats, b: &Feats) -> f64 {
    let (m, n) = (a.len(), b.len());
    if m == 0 || n == 0 { return f64::INFINITY; }
    let w = n + 1;
    let mut d = vec![f64::INFINITY; (m + 1) * w];
    let mut c = vec![0.0; m * n];
    d[0] = 0.0;
    for i in 1..=m {
        for j in 1..=n {
            let x = dist(&a[i - 1], &b[j - 1]);
            c[(i - 1) * n + j - 1] = x;
            d[i * w + j] = x + d[(i - 1) * w + j - 1].min(d[(i - 1) * w + j]).min(d[i * w + j - 1]);
        }
    }
    let (mut per, mut cnt) = (vec![0.0; m], vec![0.0; m]);
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        per[i - 1] += c[(i - 1) * n + j - 1]; cnt[i - 1] += 1.0;
        let (dd, u, l) = (d[(i - 1) * w + j - 1], d[(i - 1) * w + j], d[i * w + j - 1]);
        if dd <= u && dd <= l { i -= 1; j -= 1; } else if u <= l { i -= 1; } else { j -= 1; }
    }
    for k in 0..m { per[k] = if cnt[k] > 0.0 { per[k] / cnt[k] } else { 0.0 }; }
    let win = WINDOW.min(m);
    let mut sum: f64 = per[..win].iter().sum();
    let mut worst = sum;
    for k in win..m { sum += per[k] - per[k - win]; if sum > worst { worst = sum; } }
    worst / win as f64
}

/// How much of the loud part is voiced (a pitch).
/// How much of a consonant a clip has (V2): the highest zero-crossing rate
/// two frames running hold (20 ms frames) among its frames with any energy.
/// "Hey Cru" has the breath of its h and the burst of its k (0.2 and up); a
/// hum, voiced all through, has none (under 0.1).
pub fn consonance(clip: &[f32]) -> f64 {
    let (n, h) = (320usize, 160usize);
    if clip.len() < n * 2 { return 0.0; }
    let mut frames = vec![];
    let mut i = 0;
    while i + n <= clip.len() {
        let f = &clip[i..i + n];
        let e = f.iter().map(|x| (*x as f64).powi(2)).sum::<f64>();
        let z = f.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count() as f64 / n as f64;
        frames.push((e, z));
        i += h;
    }
    let peak = frames.iter().map(|f| f.0).fold(0.0, f64::max);
    if peak <= 0.0 { return 0.0; }
    frames.windows(2).filter(|w| w[0].0 >= peak * 0.01 && w[1].0 >= peak * 0.01).map(|w| w[0].1.min(w[1].1)).fold(0.0, f64::max)
}

/// The least consonant a wake may be: its takes' least (measured, like the
/// wake, on the audio not denoised: RNNoise takes the h's breath and the k's
/// burst for noise), with room, never under the hum floor (a hum: 0.03-0.05).
const HUM_FLOOR: f64 = 0.08;
pub fn consonance_needed(p: &Value) -> f64 { (pf(p, "consonance") * 0.6).max(HUM_FLOOR) }

pub fn voiced_share(clip: &[f32]) -> f64 {
    let (n, h) = (480, 240);
    let energy = |i: usize| (0..n).map(|k| (clip[i + k] as f64).powi(2)).sum::<f64>();
    let mut peak: f64 = 0.0;
    let mut i = 0;
    while i + n <= clip.len() { peak = peak.max(energy(i)); i += h; }
    if peak == 0.0 { return 0.0; }
    let (mut loud, mut voiced) = (0, 0);
    let mut i = 0;
    while i + n <= clip.len() {
        let e = energy(i);
        if e >= peak * 0.05 {
            loud += 1;
            let mut best: f64 = 0.0;
            for lag in 40..=228 {
                let (mut c, mut e1, mut e2) = (0.0, 0.0, 0.0);
                let mut k = 0;
                while k + lag < n { let (a, b) = (clip[i + k] as f64, clip[i + k + lag] as f64); c += a * b; e1 += a * a; e2 += b * b; k += 1; }
                let d = (e1 * e2).sqrt();
                let r = c / if d == 0.0 { 1.0 } else { d };
                if r > best { best = r; }
            }
            if best > 0.45 { voiced += 1; }
        }
        i += h;
    }
    if loud > 0 { voiced as f64 / loud as f64 } else { 0.0 }
}

/// A training recording, checked: its clip, or why to say it again.
pub fn check_sample(samples: &[f32]) -> Result<&[f32], &'static str> {
    let Some(clip) = trim(samples, 100) else { return Err("Nothing was heard. Speak a little closer.") };
    let ms = clip.len() as f64 / RATE as f64 * 1000.0;
    if ms < 350.0 { return Err("That was too short. Say the whole of \"Hey Cru\"."); }
    if ms > 2200.0 { return Err("That was too long. Say just \"Hey Cru\"."); }
    Ok(clip)
}

/// What the model hears in a clip (0.3 s before it, 1 s after).
pub fn spotted(spot: &mut dyn Spot, clip: &[f32]) -> Vec<String> {
    spot.reset();
    let mut padded = vec![0f32; RATE * 3 / 10 + clip.len() + RATE];
    padded[RATE * 3 / 10..RATE * 3 / 10 + clip.len()].copy_from_slice(clip);
    let mut names = vec![];
    for c in padded.chunks(1600) { if let Some(k) = spot.accept(c) { names.push(k); } }
    names
}

pub fn levelled(clip: &[f32]) -> Vec<f32> {
    let peak = clip.iter().fold(0f32, |p, x| p.max(x.abs()));
    if peak == 0.0 { return vec![0.0; clip.len()]; }
    let k = 0.5 / peak;
    clip.iter().map(|x| x * k).collect()
}

pub fn window_limit_of(takes: &[Feats]) -> f64 {
    if takes.len() < 2 { return 0.0; }
    let mut near: Vec<f64> = takes.iter().enumerate().map(|(i, t)| takes.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, u)| window_distance(u, t).max(window_distance(t, u))).fold(f64::INFINITY, f64::min)).collect();
    near.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (near[near.len() / 2] * WINDOW_MARGIN * 1000.0).round() / 1000.0
}

fn r3(x: f64) -> f64 { (x * 1000.0).round() / 1000.0 }

/// The training's answer: a profile, or why and which takes to say again.
pub enum Trained { Ok(Value), No { why: String, redo: Vec<usize> } }

/// Train from five recordings; `make` builds the model with keyword lines and a threshold.
pub fn train(recordings: &[Vec<f32>], make: &mut dyn FnMut(&str, f32) -> Option<Box<dyn Spot>>) -> Trained {
    train_raw(recordings, None, make)
}

/// The part of `raw` (the segment not denoised, as long as it) that `clip`
/// (a slice of `segment`) is; `clip` itself without one.
pub fn same_span<'a>(segment: &[f32], clip: &'a [f32], raw: Option<&'a [f32]>) -> &'a [f32] {
    let Some(r) = raw.filter(|r| r.len() == segment.len()) else { return clip };
    let off = (clip.as_ptr() as usize).saturating_sub(segment.as_ptr() as usize) / std::mem::size_of::<f32>();
    if off + clip.len() <= r.len() { &r[off..off + clip.len()] } else { clip }
}

/// [train], the keyword model hearing `raw`: each take not denoised (V2),
/// when every take has one as long as it.
pub fn train_raw(recordings: &[Vec<f32>], raw: Option<&[Vec<f32>]>, make: &mut dyn FnMut(&str, f32) -> Option<Box<dyn Spot>>) -> Trained {
    let raw = raw.filter(|r| r.len() == recordings.len() && r.iter().zip(recordings).all(|(a, b)| a.len() == b.len()));
    let mut clips: Vec<Vec<f32>> = vec![];
    let mut heard: Vec<Vec<f32>> = vec![];
    let mut redo = vec![];
    for (i, r) in recordings.iter().enumerate() { match check_sample(r) { Ok(c) => { heard.push(same_span(r, c, raw.map(|x| x[i].as_slice())).to_vec()); clips.push(c.to_vec()) }, Err(_) => { clips.push(vec![]); heard.push(vec![]); redo.push(i); } } }
    if !redo.is_empty() { return Trained::No { why: "Some recordings had nothing clear in them.".into(), redo }; }
    let templates: Vec<Feats> = clips.iter().map(|c| mfcc(c)).collect();
    let nearest: Vec<f64> = (0..templates.len()).map(|i| (0..templates.len()).filter(|j| *j != i).map(|j| distance(&templates[j], &templates[i])).fold(f64::INFINITY, f64::min)).collect();
    let mut sorted = nearest.clone(); sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[sorted.len() / 2];
    let odd: Vec<usize> = nearest.iter().enumerate().filter(|(_, d)| **d > median * 2.0).map(|(i, _)| i).collect();
    if !odd.is_empty() && (odd.len() as f64) < templates.len() as f64 / 2.0 {
        return Trained::No { why: "One recording sounds unlike the others. Say it again the way you normally would.".into(), redo: odd };
    }
    let spread = nearest.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mut near: Vec<f64> = (0..templates.len()).map(|i| (0..templates.len()).filter(|j| *j != i).map(|j| full_distance(&templates[j], &templates[i]).max(full_distance(&templates[i], &templates[j]))).fold(f64::INFINITY, f64::min)).collect();
    near.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let typical = near[near.len() / 2];
    let mut lens: Vec<usize> = templates.iter().map(|t| t.len()).collect(); lens.sort();
    let frames = lens[lens.len() / 2];

    // The decoys the owner's own "Hey Cru" sets off are dropped (up to three rounds).
    let mut dropped: std::collections::HashSet<&str> = Default::default();
    for _ in 0..3 {
        let use_: Vec<(&str, &str)> = DECOYS.iter().copied().filter(|(n, _)| !dropped.contains(n)).collect();
        let lines = competing_lines(CONFIRM_THRESHOLD, KEYWORDS_SCORE, &use_);
        let Some(mut spot) = make(&lines, CONFIRM_THRESHOLD) else { return Trained::No { why: "The keyword spotter could not be created (model files missing or damaged)".into(), redo: vec![] } };
        let mut more = false;
        for c in &heard {
            for k in spotted(spot.as_mut(), &levelled(c)) {
                if let Some(n) = k.strip_prefix("NOT_") { if let Some((name, _)) = DECOYS.iter().find(|(x, _)| *x == n) { if dropped.insert(name) { more = true; } } }
            }
        }
        if !more { break; }
    }
    let decoys: Vec<(&str, &str)> = DECOYS.iter().copied().filter(|(n, _)| !dropped.contains(n)).collect();

    // The strictest threshold that hears the most takes, then two notches looser.
    let (mut best, mut most) = (THRESHOLDS.len() - 1, 0);
    for (t, th) in THRESHOLDS.iter().enumerate() {
        let Some(mut spot) = make(&competing_lines(*th, KEYWORDS_SCORE, &decoys), *th) else { continue };
        let n = heard.iter().filter(|c| spotted(spot.as_mut(), c).iter().any(|k| k == WAKE)).count();
        if n > most { most = n; best = t; }
        if n == clips.len() { break; }
    }
    let threshold = THRESHOLDS[(best + 2).min(THRESHOLDS.len() - 1)];
    let mut all: Vec<Feats> = templates.clone();
    for c in &clips { for v in variants(c) { all.push(mfcc(&v)); } }
    let tpl: Vec<Vec<Vec<f64>>> = all.iter().map(|t| t.iter().map(|f| f.iter().map(|x| r3(*x)).collect()).collect()).collect();
    Trained::Ok(json!({
        "version": 4,
        "keyword": keyword_lines(threshold, KEYWORDS_SCORE),
        "keywordsScore": KEYWORDS_SCORE,
        "keywordsThreshold": threshold,
        "limit": r3(spread * LIMIT_MARGIN),
        "strictLimit": r3(typical * STRICT_MARGIN),
        "takes": templates.len(),
        "phraseFrames": frames,
        "modelHeard": most,
        "decoys": decoys.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        "windowLimit": window_limit_of(&templates),
        "consonance": r3(heard.iter().map(|c| consonance(c)).fold(f64::INFINITY, f64::min)),
        "templates": tpl,
        "trainedAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    }))
}

pub fn valid_profile(p: &Value) -> bool {
    p.get("version").and_then(|v| v.as_i64()) == Some(4) && pf(p, "phraseFrames") > 0.0
        && p.get("templates").and_then(|t| t.as_array()).is_some_and(|t| t.len() >= 3)
        && pf(p, "limit") > 0.0 && pf(p, "keywordsThreshold") > 0.0 && p.get("keyword").is_some_and(|k| k.is_string())
}

/// A profile's templates, as features.
pub fn templates_of(p: &Value) -> Vec<Feats> {
    p.get("templates").and_then(|t| t.as_array()).map(|a| a.iter().map(|t| t.as_array().map(|fs| fs.iter().map(|f| f.as_array().map(|x| x.iter().filter_map(|v| v.as_f64()).collect()).unwrap_or_default()).collect()).unwrap_or_default()).collect()).unwrap_or_default()
}

pub fn verify_limit(p: &Value) -> f64 { pf(p, "limit") / LIMIT_MARGIN * STRICT_MARGIN }
/// The strict limit a phrase must meet, from the profile's limit (when it has no strictLimit of its own).
pub fn strict_limit_of(limit: f64) -> f64 { limit / LIMIT_MARGIN * STRICT_MARGIN }
pub fn strict_limit(p: &Value) -> f64 { if pf(p, "strictLimit") > 0.0 { pf(p, "strictLimit") } else { pf(p, "limit") / LIMIT_MARGIN * STRICT_MARGIN } }
pub fn window_limit(p: &Value, tpl: &[Feats]) -> f64 {
    if pf(p, "windowLimit") > 0.0 { return pf(p, "windowLimit"); }
    let takes = pf(p, "takes") as usize;
    if takes >= 2 { window_limit_of(&tpl[..takes.min(tpl.len())]) } else { 0.0 }
}

/// The model's hit checked against the owner's phrase (the last VERIFY_SECONDS).
pub fn verify(p: &Value, tpl: &[Feats], recent: &[f32]) -> (bool, f64) {
    let clip = trim(recent, 60).unwrap_or(recent);
    let feats = mfcc(clip);
    let best = tpl.iter().map(|t| distance(t, &feats)).fold(f64::INFINITY, f64::min);
    (best <= verify_limit(p), r3(best))
}

/// A burst matched against the owner's phrase; `confirm` is the model's second opinion.
pub fn match_phrase(p: &Value, tpl: &[Feats], segment: &[f32], confirm: Option<&mut dyn FnMut(&[f32]) -> Vec<String>>) -> (bool, f64, &'static str) {
    match_phrase_raw(p, tpl, segment, None, confirm)
}

/// [match_phrase], the model's second opinion on `raw`: the same segment
/// not denoised (V2), the same samples as the trimmed clip.
pub fn match_phrase_raw(p: &Value, tpl: &[Feats], segment: &[f32], raw: Option<&[f32]>, confirm: Option<&mut dyn FnMut(&[f32]) -> Vec<String>>) -> (bool, f64, &'static str) {
    let Ok(clip) = check_sample(segment) else { return (false, f64::INFINITY, "length") };
    let heard = same_span(segment, clip, raw);
    let frames = pf(p, "phraseFrames");
    let len = (clip.len() as f64 - 400.0) / 160.0 + 1.0;
    if len < frames * LENGTH_RANGE.0 || len > frames * LENGTH_RANGE.1 { return (false, f64::INFINITY, "length"); }
    if voiced_share(clip) < MIN_VOICED { return (false, f64::INFINITY, "not speech"); }
    let feats = mfcc(clip);
    let best = tpl.iter().map(|t| full_distance(t, &feats).max(full_distance(&feats, t))).fold(f64::INFINITY, f64::min);
    let score = r3(best);
    if best > strict_limit(p) { return (false, score, "unlike"); }
    let wl = window_limit(p, tpl);
    if wl > 0.0 {
        let w = tpl.iter().map(|t| window_distance(t, &feats).max(window_distance(&feats, t))).fold(f64::INFINITY, f64::min);
        if w > wl { return (false, score, "onset"); }
    }
    // V2, the owner: "the way I say Hey Cru is what it should train on".
    // Where the model hears this voice (most of the takes), it must hear
    // "Hey Cru" here too and no decoy beside it. Where it doesn't, its word
    // is no veto: the owner's own takes decide (the phrase, its onset, and
    // the voice check after this), and the hum a model-less match once let
    // through (12 Nov) is kept out by the consonants his takes have.
    let trusted = p.get("modelHeard").and_then(|v| v.as_i64()).unwrap_or(0) >= MODEL_TRUSTED;
    if trusted {
        if let Some(c) = confirm {
            let names = c(&levelled(heard));
            if !names.iter().any(|k| k == WAKE) { return (false, score, if names.is_empty() { "no word" } else { "decoy" }); }
        }
    } else if consonance(heard) < consonance_needed(p) {
        return (false, score, "hum");
    }
    (true, score, "")
}

/// Bursts of sound (a phrase) out of the stream: each one when a short quiet ends it.
#[derive(Default)]
pub struct Segmenter { floor: f64, active: bool, long: bool, quiet_ms: f64, loud_ms: f64, parts: Vec<Vec<f32>>, pre: Option<Vec<f32>> }

impl Segmenter {
    pub fn new() -> Self { Segmenter { floor: 0.002, ..Default::default() } }
    pub fn accept(&mut self, samples: &[f32]) -> Option<Vec<f32>> {
        const MIN_RMS: f64 = 0.006; const END_QUIET_MS: f64 = 300.0; const MAX_MS: f64 = 2500.0;
        let rms = (samples.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt();
        let ms = samples.len() as f64 / RATE as f64 * 1000.0;
        let loud = rms > MIN_RMS.max(self.floor * 3.0);
        if !loud { self.floor = if rms < self.floor { self.floor * 0.7 + rms * 0.3 } else { self.floor * 0.995 + rms * 0.005 }; }
        if !self.active {
            if loud { self.active = true; self.long = false; self.quiet_ms = 0.0; self.loud_ms = ms; self.parts = match self.pre.take() { Some(p) => vec![p, samples.to_vec()], None => vec![samples.to_vec()] }; }
            else { self.pre = Some(samples.to_vec()); }
            return None;
        }
        self.parts.push(samples.to_vec());
        if loud { self.quiet_ms = 0.0; self.loud_ms += ms; } else { self.quiet_ms += ms; }
        if self.loud_ms + self.quiet_ms > MAX_MS { self.long = true; self.parts.clear(); }
        if self.quiet_ms < END_QUIET_MS { return None; }
        self.active = false; self.pre = None;
        if self.long { return None; }
        Some(std::mem::take(&mut self.parts).concat())
    }
    pub fn reset(&mut self) { let f = self.floor; *self = Segmenter { floor: f, ..Default::default() }; }
}

#[cfg(test)]
mod tests {
    /// V2: a hum has no consonant; a burst of breath and a click has.
    #[test]
    fn consonance_tells_a_hum() {
        let hum: Vec<f32> = (0..RATE * 6 / 10).map(|i| { let t = i as f32 / RATE as f32; let env = (std::f32::consts::PI * t / 0.6).sin(); env * 0.3 * ((2.0 * std::f32::consts::PI * 150.0 * t).sin() + 0.5 * (2.0 * std::f32::consts::PI * 300.0 * t).sin() + 0.25 * (2.0 * std::f32::consts::PI * 450.0 * t).sin()) }).collect();
        assert!(consonance(&hum) < HUM_FLOOR, "a hum: {}", consonance(&hum));
        let mut seed = 1u32;
        let mut noise = || { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.2 };
        let breath: Vec<f32> = (0..RATE / 10).map(|_| noise()).collect();
        let word = [breath, hum.clone()].concat();
        assert!(consonance(&word) >= HUM_FLOOR, "breath then voice: {}", consonance(&word));
    }

    use super::*;

    fn tone(ms: usize, hz: f32, amp: f32) -> Vec<f32> { (0..RATE * ms / 1000).map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin()).collect() }
    fn silence(ms: usize) -> Vec<f32> { vec![0.0; RATE * ms / 1000] }

    #[test]
    fn lines() {
        assert_eq!(keyword_lines(0.2, 1.5).lines().next().unwrap(), "▁HE Y ▁C RU :1.5 #0.2 @HEY_CRU");
        assert!(competing_lines(0.04, 1.5, &DECOYS[..1]).ends_with("▁HE Y ▁YOU :1.5 #0.04 @NOT_YOU"));
    }

    #[test]
    fn checks_samples() {
        assert_eq!(check_sample(&silence(1000)).unwrap_err(), "Nothing was heard. Speak a little closer.");
        let short = [silence(300), tone(100, 200.0, 0.3), silence(300)].concat();
        assert!(check_sample(&short).unwrap_err().contains("too short"));
        let long = [silence(300), tone(3000, 200.0, 0.3), silence(300)].concat();
        assert!(check_sample(&long).unwrap_err().contains("too long"));
        let ok = [silence(300), tone(700, 200.0, 0.3), silence(300)].concat();
        assert!(check_sample(&ok).is_ok());
    }

    #[test]
    fn features_and_distances() {
        let a = mfcc(&tone(600, 220.0, 0.3));
        assert_eq!(a[0].len(), 24);
        assert!(distance(&a, &a) < 1e-9);
        assert!(full_distance(&a, &a) < 1e-9);
        // Steady tones are all "noise" to the spectral subtraction: a change of pitch is a phrase.
        let x = mfcc(&[tone(300, 220.0, 0.3), tone(300, 660.0, 0.3)].concat());
        let y = mfcc(&[tone(300, 990.0, 0.3), tone(300, 330.0, 0.3)].concat());
        assert!(full_distance(&x, &y) > 0.1, "{}", full_distance(&x, &y));
        assert!(voiced_share(&tone(600, 150.0, 0.3)) > 0.9);
    }

    #[test]
    fn segments() {
        let mut s = Segmenter::new();
        let mut out = None;
        for c in [silence(500), tone(600, 200.0, 0.3), silence(500)].concat().chunks(1600) { if let Some(x) = s.accept(c) { out = Some(x); } }
        let n = out.expect("a burst").len();
        assert!(n >= RATE * 600 / 1000 && n < RATE * 1300 / 1000, "{n}");
    }

    #[test]
    fn validates_profiles() {
        assert!(!valid_profile(&json!({})));
        assert!(valid_profile(&json!({ "version": 4, "phraseFrames": 50, "templates": [[[0.0]], [[0.0]], [[0.0]]], "limit": 1.0, "keywordsThreshold": 0.2, "keyword": "x" })));
    }
}
