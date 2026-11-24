//! The wake word's engine: sherpa-onnx's keyword spotter, compiled in
//! (the sherpa-onnx crate, static), on the model the PC client uses
//! (app/voice-wake/manifest.js MODEL), downloaded once into the config folder
//! and checked against its hashes (install.js); the spotter with its noise
//! gate (spotter.js); `Wake`, the two ways in (worker.js createWake); the
//! owner's voice (owner.js, voiceprint.js) on sherpa-onnx's speaker
//! embedding with the same 3D-Speaker model.
//!
//! CRUNDI_WAKE_MODELS=<dir> uses models already there (the tests; read only).

use super::enroll::{self, Feats, Spot, RATE, VERIFY_SECONDS, WAKE};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig, OnlineStream, SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig};
use std::path::{Path, PathBuf};

pub const MODEL_ID: &str = "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01";
const MODEL_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01.tar.bz2";
const MODEL_SHA: &str = "f170013b4716e41b62b9bfd809687c207cef798ef9bc6534d524e17af9b6561a";
const ENCODER: &str = "encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx";
const DECODER: &str = "decoder-epoch-12-avg-2-chunk-16-left-64.onnx";
const JOINER: &str = "joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx";
const TOKENS: &str = "tokens.txt";
const FILES: &[(&str, &str)] = &[
    (ENCODER, "1e721676515bcd42a186979733981213c66c80db680e1cc582dfedf3be76e678"),
    (DECODER, "f61ebd3eed3773a44d088d53dfae92dbb6aec4839f4dcaee2d402414741663a3"),
    (JOINER, "eae9da0c7e1e6c6a3f4cc42d167899c388f6c6701b94cb96320e4f55df79624c"),
    (TOKENS, "fd2ded4050a55d2b1578870ba8697d02371980217806b7558bd0a5cc60f3ba53"),
];
/// The owner's voice model (voiceprint.js MODEL).
const VOICE_FILE: &str = "3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx";
const VOICE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx";
const VOICE_SHA: &str = "c4947caf150b8502fdd75bbe4089b971ab1ec42e90ee3ba547faca2cde44d9ee";
pub const VOICE_MODEL: &str = "eres2net-voxceleb";
pub const VOICE_DIM: usize = 192;
const OWNER_THRESHOLD: f32 = 0.42;
const OWNER_K: usize = 3;
const OWNER_MIN_SECONDS: f32 = 0.3;
/// One wake per utterance (TUNING.cooldownMs).
pub const COOLDOWN_MS: u64 = 2500;

/// Where the models are (the config folder's voice-wake, or CRUNDI_WAKE_MODELS).
pub fn models_dir() -> PathBuf {
    std::env::var_os("CRUNDI_WAKE_MODELS").map(PathBuf::from).unwrap_or_else(|| crundi_core::store::config_dir().join("voice-wake"))
}

fn sha_file(p: &Path) -> Option<String> {
    let b = std::fs::read(p).ok()?;
    Some(hex(&Sha256::digest(&b)))
}
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }

/// The keyword model is in place (its marker, or its files checked).
pub fn installed() -> bool {
    let d = models_dir().join(MODEL_ID);
    if std::fs::read_to_string(d.join(".ok")).is_ok_and(|s| s.trim() == MODEL_SHA) { return true; }
    FILES.iter().all(|(f, _)| d.join(f).exists()) && std::env::var_os("CRUNDI_WAKE_MODELS").is_some()
}

fn download(url: &str, to: &Path) -> Result<String, String> {
    let r = ureq::get(url).call().map_err(|e| format!("Download failed: {e}"))?;
    let mut body = r.into_body();
    let mut reader = body.as_reader();
    let mut f = std::fs::File::create(to).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        use std::io::{Read, Write};
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 { break; }
        h.update(&buf[..n]);
        f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    Ok(hex(&h.finalize()))
}

/// Download the keyword model once, checked (ensureInstalled); the owner's
/// voice model too (its failure only turns the voice check off).
pub fn install() -> Result<(), String> {
    // One at a time: training's own install waits for the one Train started (two at once would
    // write the same download and unpack into the same folder).
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let base = models_dir();
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    if !installed() {
        let tmp = base.join(format!("{MODEL_ID}.download"));
        let got = download(MODEL_URL, &tmp)?;
        if got != MODEL_SHA { let _ = std::fs::remove_file(&tmp); return Err(format!("The download of {MODEL_ID} did not match its checksum (got {got})")); }
        let stage = base.join(format!("{MODEL_ID}.staging"));
        let _ = std::fs::remove_dir_all(&stage);
        let file = std::fs::File::open(&tmp).map_err(|e| e.to_string())?;
        tar::Archive::new(bzip2::read::BzDecoder::new(file)).unpack(&stage).map_err(|e| format!("Could not unpack the model: {e}"))?;
        let dst = base.join(MODEL_ID);
        let _ = std::fs::remove_dir_all(&dst);
        std::fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
        for (f, want) in FILES {
            let src = stage.join(MODEL_ID).join(f);
            if sha_file(&src).as_deref() != Some(*want) { return Err(format!("{f} did not match its checksum")); }
            std::fs::rename(&src, dst.join(f)).or_else(|_| std::fs::copy(&src, dst.join(f)).map(|_| ())).map_err(|e| e.to_string())?;
        }
        std::fs::write(dst.join(".ok"), MODEL_SHA).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_dir_all(&stage);
    }
    let vf = base.join("speaker").join(VOICE_FILE);
    if !vf.exists() {
        let _ = std::fs::create_dir_all(base.join("speaker"));
        let tmp = vf.with_extension("part");
        match download(VOICE_URL, &tmp) {
            Ok(got) if got == VOICE_SHA => { let _ = std::fs::rename(&tmp, &vf); }
            _ => { let _ = std::fs::remove_file(&tmp); eprintln!("[voice-wake] voice model: not downloaded"); }
        }
    }
    Ok(())
}

// ─── The spotter (spotter.js) ───

pub struct Spotter { kws: KeywordSpotter, stream: OnlineStream, gate: bool, floor: f64, open: bool, quiet_ms: f64, pre: Vec<Vec<f32>>, pre_len: usize }

impl Spotter {
    /// The model with these keyword lines, its score and threshold; `gate`: the noise gate (off in training).
    pub fn new(keywords: &str, score: f32, threshold: f32, gate: bool) -> Result<Spotter, String> {
        let d = models_dir().join(MODEL_ID);
        let p = |f: &str| Some(d.join(f).to_string_lossy().to_string());
        let mut cfg = KeywordSpotterConfig::default();
        cfg.model_config.transducer.encoder = p(ENCODER);
        cfg.model_config.transducer.decoder = p(DECODER);
        cfg.model_config.transducer.joiner = p(JOINER);
        cfg.model_config.tokens = p(TOKENS);
        cfg.model_config.provider = Some("cpu".into());
        cfg.model_config.num_threads = 1;
        cfg.model_config.modeling_unit = Some("bpe".into());
        cfg.max_active_paths = 4;
        cfg.num_trailing_blanks = 1;
        cfg.keywords_score = score;
        cfg.keywords_threshold = threshold;
        cfg.keywords_buf = Some(keywords.to_string());
        let kws = KeywordSpotter::create(&cfg).ok_or("The keyword spotter could not be created (model files missing or damaged)")?;
        let stream = kws.create_stream();
        Ok(Spotter { kws, stream, gate, floor: 0.002, open: false, quiet_ms: 0.0, pre: vec![], pre_len: 0 })
    }

    fn decode(&mut self, samples: &[f32]) -> Option<String> {
        self.stream.accept_waveform(RATE as i32, samples);
        let mut hit = None;
        while self.kws.is_ready(&self.stream) {
            self.kws.decode(&self.stream);
            if let Some(r) = self.kws.get_result(&self.stream) { if !r.keyword.is_empty() { hit = Some(r.keyword); self.kws.reset(&self.stream); } }
        }
        hit
    }
}

impl Spot for Spotter {
    /// A block in: the keyword it heard (the gate decodes only around sound, with half a second before).
    fn accept(&mut self, samples: &[f32]) -> Option<String> {
        if !self.gate { return self.decode(samples); }
        const PRE: usize = RATE / 2; const HANG_MS: f64 = 1500.0; const MIN_RMS: f64 = 0.0025;
        let rms = (samples.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / samples.len().max(1) as f64).sqrt();
        let loud = rms > MIN_RMS.max(self.floor * 3.0);
        if !loud { self.floor = if rms < self.floor { self.floor * 0.7 + rms * 0.3 } else { self.floor * 0.995 + rms * 0.005 }; }
        let ms = samples.len() as f64 / RATE as f64 * 1000.0;
        if loud {
            self.quiet_ms = 0.0;
            if !self.open {
                self.open = true;
                let pre = std::mem::take(&mut self.pre); self.pre_len = 0;
                let mut hit = None;
                for p in pre { hit = self.decode(&p).or(hit); }
                return self.decode(samples).or(hit);
            }
            return self.decode(samples);
        }
        if self.open {
            self.quiet_ms += ms;
            let hit = self.decode(samples);
            if self.quiet_ms >= HANG_MS {
                self.open = false;
                let tail = self.decode(&vec![0.0; RATE / 2]);
                self.stream = self.kws.create_stream();
                return hit.or(tail);
            }
            return hit;
        }
        self.pre.push(samples.to_vec());
        self.pre_len += samples.len();
        while self.pre.len() > 1 && self.pre_len - self.pre[0].len() >= PRE { self.pre_len -= self.pre.remove(0).len(); }
        None
    }
    fn reset(&mut self) { self.open = false; self.quiet_ms = 0.0; self.pre.clear(); self.pre_len = 0; self.stream = self.kws.create_stream(); }
}

/// The trainer's model maker: no gate.
pub fn maker() -> impl FnMut(&str, f32) -> Option<Box<dyn Spot>> {
    |lines: &str, th: f32| Spotter::new(lines, enroll::KEYWORDS_SCORE, th, false).ok().map(|s| Box::new(s) as Box<dyn Spot>)
}

// ─── The owner's voice (owner.js, voiceprint.js) ───

pub struct Owner { ex: SpeakerEmbeddingExtractor }

fn normalise(v: &[f32]) -> Vec<f32> { let s = v.iter().map(|x| x * x).sum::<f32>().sqrt(); let k = if s > 0.0 { 1.0 / s } else { 1.0 }; v.iter().map(|x| x * k).collect() }
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let (d, x, y) = a.iter().zip(b).fold((0f32, 0f32, 0f32), |(d, x, y), (p, q)| (d + p * q, x + p * p, y + q * q));
    let n = (x * y).sqrt(); d / if n > 0.0 { n } else { 1.0 }
}
/// The mean of the best k matches (wordScore).
pub fn word_score(v: &[f32], clips: &[Vec<f32>], k: usize) -> f32 {
    if clips.is_empty() { return f32::NAN; }
    let mut c: Vec<f32> = clips.iter().map(|x| cosine(v, x)).collect();
    c.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let n = k.min(c.len());
    c[..n].iter().sum::<f32>() / n as f32
}

pub fn valid_voice(v: &Value) -> bool {
    v.get("model").and_then(|m| m.as_str()) == Some(VOICE_MODEL)
        && v.get("clips").and_then(|c| c.as_array()).is_some_and(|c| c.len() >= OWNER_K && c.iter().all(|x| x.as_array().is_some_and(|a| a.len() == VOICE_DIM)))
}

impl Owner {
    pub fn load() -> Option<Owner> {
        let f = models_dir().join("speaker").join(VOICE_FILE);
        if !f.exists() { return None; }
        let cfg = SpeakerEmbeddingExtractorConfig { model: Some(f.to_string_lossy().to_string()), num_threads: 1, ..Default::default() };
        SpeakerEmbeddingExtractor::create(&cfg).map(|ex| Owner { ex })
    }
    pub fn embed(&self, clip: &[f32]) -> Option<Vec<f32>> {
        let s = self.ex.create_stream()?;
        s.accept_waveform(RATE as i32, clip);
        s.input_finished();
        if !self.ex.is_ready(&s) { return None; }
        self.ex.compute(&s).map(|e| normalise(&e))
    }
    /// The owner's voice from the training clips.
    pub fn enrol(&self, clips: &[Vec<f32>]) -> Option<Value> {
        let out: Option<Vec<Vec<f32>>> = clips.iter().map(|c| self.embed(c).map(|e| e.iter().map(|x| (x * 1e4).round() / 1e4).collect())).collect();
        out.map(|o| serde_json::json!({ "model": VOICE_MODEL, "clips": o }))
    }
    /// Is this the owner? Some(score) at the threshold or over; None: too short or no answer.
    pub fn check(&self, voice: &Value, clip: &[f32]) -> (Option<bool>, f32) {
        if !valid_voice(voice) || (clip.len() as f32 / RATE as f32) < OWNER_MIN_SECONDS { return (None, 0.0); }
        let Some(e) = self.embed(clip) else { return (None, 0.0) };
        let clips: Vec<Vec<f32>> = voice["clips"].as_array().unwrap().iter().map(|c| c.as_array().unwrap().iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()).collect();
        let s = (word_score(&e, &clips, OWNER_K) * 1000.0).round() / 1000.0;
        (Some(s >= OWNER_THRESHOLD), s)
    }
}

// ─── The wake (worker.js createWake) ───

/// A wake: how it was heard, its score, and the clip the owner's voice is checked on.
/// The microphone's mono samples at its own rate to 16 kHz (mic), as
/// Chromium's AudioContext({ sampleRate: 16000 }) resamples Electron's: a
/// low-pass (63-tap windowed sinc, cut at 7 kHz) first, then the 16 kHz
/// points between its samples. (V2: the mean of each step it used to take
/// folded everything over 8 kHz back into the band, which the keyword model
/// does not hear "Hey Cru" through.)
pub struct To16 { step: f64, pos: f64, buf: Vec<f32>, taps: Vec<f32> }

impl To16 {
    pub fn new(rate: usize) -> To16 {
        const N: usize = 63;
        let fc = 7000.0 / rate as f64;
        let m = (N - 1) as f64 / 2.0;
        let mut taps: Vec<f64> = (0..N).map(|k| {
            let x = k as f64 - m;
            let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * x) };
            sinc * (0.54 - 0.46 * (2.0 * std::f64::consts::PI * k as f64 / (N - 1) as f64).cos())
        }).collect();
        let sum: f64 = taps.iter().sum();
        for t in &mut taps { *t /= sum; }
        To16 { step: rate as f64 / RATE as f64, pos: (N - 1) as f64, buf: vec![0.0; N - 1], taps: taps.into_iter().map(|t| t as f32).collect() }
    }
    fn at(&self, i: usize) -> f32 { self.taps.iter().enumerate().map(|(k, t)| t * self.buf[i - k]).sum() }
    pub fn push(&mut self, b: &[f32]) -> Vec<f32> {
        self.buf.extend_from_slice(b);
        let mut o = vec![];
        while self.pos + 1.0 < self.buf.len() as f64 {
            let i = self.pos as usize;
            let f = (self.pos - i as f64) as f32;
            let (a, c) = (self.at(i), self.at(i + 1));
            o.push(a + (c - a) * f);
            self.pos += self.step;
        }
        // Keep what the next samples' filter still reaches back to.
        let keep = (self.pos as usize).saturating_sub(self.taps.len() - 1).min(self.buf.len());
        self.buf.drain(..keep);
        self.pos -= keep as f64;
        o
    }
}

/// The phrase match a wake needs (the profile's limit; lower is closer).
fn profile_limit(p: &Value) -> f64 { p["limit"].as_f64().unwrap_or(0.0) }

pub struct Heard { pub ok: bool, pub keyword: String, pub score: f64, pub via: &'static str, pub clip: Vec<f32> }

pub struct Wake {
    profile: Value,
    tpl: Vec<Feats>,
    spot: Spotter,
    seg: enroll::Segmenter,
    loose: Option<Spotter>,
    ring: Vec<f32>,
    filled: usize,
    at: usize,
    last_hit: Option<std::time::Instant>,
    /// The last few seconds not denoised (V2): what the keyword model hears.
    raw_hist: Vec<f32>,
    /// The last phrase that did not wake but came within twice its limit:
    /// (its match, the limit, the gate that refused it, the burst at 16 kHz).
    pub near_miss: Option<(f64, f64, &'static str, Vec<f32>)>,
    /// The near miss's burst not denoised.
    pub last_raw: Option<Vec<f32>>,
}

impl Wake {
    pub fn new(profile: &Value) -> Result<Wake, String> {
        if !enroll::valid_profile(profile) { return Err("Hey Cru is not trained".into()); }
        let spot = Spotter::new(&enroll::listen_lines(profile), profile["keywordsScore"].as_f64().unwrap_or(1.5) as f32, profile["keywordsThreshold"].as_f64().unwrap_or(0.2) as f32, true)?;
        Ok(Wake { tpl: enroll::templates_of(profile), profile: profile.clone(), spot, seg: enroll::Segmenter::new(), loose: None, ring: vec![0.0; (VERIFY_SECONDS * RATE as f32) as usize], filled: 0, at: 0, last_hit: None, raw_hist: vec![], near_miss: None, last_raw: None })
    }

    fn push(&mut self, s: &[f32]) {
        let n = self.ring.len();
        if s.len() >= n { self.ring.copy_from_slice(&s[s.len() - n..]); self.at = 0; self.filled = n; return; }
        let first = s.len().min(n - self.at);
        self.ring[self.at..self.at + first].copy_from_slice(&s[..first]);
        if first < s.len() { self.ring[..s.len() - first].copy_from_slice(&s[first..]); }
        self.at = (self.at + s.len()) % n;
        self.filled = n.min(self.filled + s.len());
    }
    fn recent(&self) -> Vec<f32> {
        if self.filled < self.ring.len() { self.ring[..self.filled].to_vec() } else { [&self.ring[self.at..], &self.ring[..self.at]].concat() }
    }

    /// A block of 16 kHz samples in: a wake (or a refused one), or nothing.
    pub fn accept(&mut self, samples: &[f32]) -> Option<Heard> { self.accept_both(samples, samples) }

    /// A block in, cleaned (the phrase, its match, the owner's voice) and
    /// the same block not denoised (V2: the keyword model's, as Electron's
    /// model heard Chromium's audio, not RNNoise's).
    pub fn accept_both(&mut self, samples: &[f32], raw: &[f32]) -> Option<Heard> {
        let raw = if raw.len() == samples.len() { raw } else { samples };
        self.push(samples);
        self.raw_hist.extend_from_slice(raw);
        let keep = RATE * 4;
        if self.raw_hist.len() > keep { let cut = self.raw_hist.len() - keep; self.raw_hist.drain(..cut); }
        let k = self.spot.accept(raw);
        let burst = self.seg.accept(samples);
        if self.last_hit.is_some_and(|t| t.elapsed().as_millis() as u64 <= COOLDOWN_MS) { return None; }
        if let Some(b) = burst {
            let (profile, tpl) = (self.profile.clone(), std::mem::take(&mut self.tpl));
            let loose = &mut self.loose;
            let mut confirm = |clip: &[f32]| -> Vec<String> {
                if loose.is_none() { *loose = Spotter::new(&enroll::confirm_lines(&profile), profile["keywordsScore"].as_f64().unwrap_or(1.5) as f32, enroll::CONFIRM_THRESHOLD, false).ok(); }
                match loose.as_mut() { Some(s) => enroll::spotted(s, clip), None => vec![] }
            };
            // The burst ends with this block: its undenoised twin, the same last samples.
            let raw_b = (b.len() <= self.raw_hist.len()).then(|| self.raw_hist[self.raw_hist.len() - b.len()..].to_vec());
            let (ok, score, why) = enroll::match_phrase_raw(&profile, &tpl, &b, raw_b.as_deref(), Some(&mut confirm));
            self.tpl = tpl;
            let limit = profile_limit(&self.profile);
            if !ok && score.is_finite() && limit > 0.0 && score < limit * 2.0 { self.near_miss = Some((score, limit, why, b.clone())); self.last_raw = raw_b.clone(); }
            if ok {
                self.last_hit = Some(std::time::Instant::now());
                let clip = enroll::trim(&b, 100).map(|c| c.to_vec()).unwrap_or(b);
                return Some(Heard { ok: true, keyword: WAKE.into(), score, via: "phrase", clip });
            }
        }
        let k = k?;
        if k != WAKE { return None; }
        let heard = self.recent();
        let (ok, score) = enroll::verify(&self.profile, &self.tpl, &heard);
        if ok { self.last_hit = Some(std::time::Instant::now()); }
        let clip = enroll::trim(&heard, 60).map(|c| c.to_vec()).unwrap_or(heard);
        Some(Heard { ok, keyword: k, score, via: "model", clip })
    }

    /// The owner's voice refused it: the cooldown does not hide the next.
    pub fn refused(&mut self) { self.last_hit = None; }
    pub fn reset(&mut self) { self.spot.reset(); self.seg.reset(); self.filled = 0; self.at = 0; self.raw_hist.clear(); }
}

/// Train from five recordings, with the owner's voice when its model is there.
pub fn train_with_voice(recordings: &[Vec<f32>]) -> Result<(Value, String), (String, Vec<usize>)> { train_with_voice_raw(recordings, None) }

/// [train_with_voice], the keyword model hearing the takes not denoised (V2).
pub fn train_with_voice_raw(recordings: &[Vec<f32>], raw: Option<&[Vec<f32>]>) -> Result<(Value, String), (String, Vec<usize>)> {
    let mut make = maker();
    match enroll::train_raw(recordings, raw, &mut make) {
        enroll::Trained::No { why, redo } => Err((why, redo)),
        enroll::Trained::Ok(mut p) => {
            let clips: Vec<Vec<f32>> = recordings.iter().filter_map(|r| enroll::check_sample(r).ok().map(|c| c.to_vec())).collect();
            let note = match Owner::load().and_then(|o| o.enrol(&clips)) {
                Some(v) => { p["voice"] = v; String::new() }
                None => "The voice check could not be set up (the voice model is missing): Hey Cru works, for any voice, until it is trained again.".to_string(),
            };
            Ok((p, note))
        }
    }
}

/// A 16-bit mono WAV's samples (the tests' recordings), resampled to 16 kHz when needed.
pub fn read_wav(b: &[u8]) -> Option<Vec<f32>> {
    let (rate, s) = crundi_core::voice::wav(b)?;
    if rate as usize == RATE { return Some(s); }
    let k = rate as f64 / RATE as f64;
    Some((0..(s.len() as f64 / k) as usize).map(|i| s[((i as f64 * k) as usize).min(s.len() - 1)]).collect())
}

#[cfg(test)]
mod tests {
    /// V2: To16 keeps speech (1 kHz) at its level, removes what would fold
    /// back into the band (12 kHz from 48 kHz), and gives 16 000 a second
    /// whatever the blocks.
    #[test]
    fn to16_low_passes_and_keeps_the_rate() {
        let tone = |hz: f64, rate: usize, n: usize| -> Vec<f32> { (0..n).map(|i| (2.0 * std::f64::consts::PI * hz * i as f64 / rate as f64).sin() as f32 * 0.5).collect() };
        let rms = |x: &[f32]| (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt();
        for rate in [48_000usize, 44_100] {
            let mut d = super::To16::new(rate);
            let mut out = vec![];
            for b in tone(1000.0, rate, rate).chunks(441) { out.extend(d.push(b)); }
            assert!((out.len() as i64 - 16_000).abs() <= 64, "{rate}: {} samples", out.len());
            let r = rms(&out[1000..]);
            assert!((r - 0.5 / 2f64.sqrt()).abs() < 0.02, "{rate}: speech kept at {r}");
            let mut d = super::To16::new(rate);
            let hi: Vec<f32> = tone(12_000.0, rate, rate).chunks(441).flat_map(|b| d.push(b)).collect();
            assert!(rms(&hi[1000..]) < 0.01, "{rate}: 12 kHz folded back at {}", rms(&hi[1000..]));
        }
    }

    use super::*;

    /// Trained on one voice's five takes: its "Hey Cru" wakes, its decoys do
    /// not (scripts/test-voice-wake-train.mjs). Needs the recordings
    /// (VW_EVAL_DIR, /tmp/cru-eval) and the models (VW_INSTALL_DIR,
    /// /tmp/vw-install); skipped without them.
    #[test]
    fn trains_and_wakes_on_real_recordings() {
        let eval = std::env::var("VW_EVAL_DIR").unwrap_or_else(|_| "/tmp/cru-eval".into());
        let models = std::env::var("VW_INSTALL_DIR").unwrap_or_else(|_| "/tmp/vw-install".into());
        let wav = |n: &str| std::fs::read(format!("{eval}/{n}.wav")).ok().and_then(|b| read_wav(&b));
        let recs: Option<Vec<Vec<f32>>> = (1..=5).map(|i| wav(&format!("draco-train{i}"))).collect();
        let Some(recs) = recs.filter(|_| Path::new(&models).join(MODEL_ID).exists()) else { eprintln!("skip: no recordings or models"); return };
        unsafe { std::env::set_var("CRUNDI_WAKE_MODELS", &models); }
        let (profile, _) = train_with_voice(&recs).unwrap_or_else(|(why, redo)| panic!("training: {why} {redo:?}"));
        assert!(enroll::valid_profile(&profile));
        let heard = |n: &str| -> bool {
            let Some(s) = wav(n) else { return false };
            let mut w = Wake::new(&profile).unwrap();
            let mut padded = vec![0f32; RATE / 2];
            padded.extend(s);
            padded.extend(vec![0f32; RATE]);
            padded.chunks(1600).any(|c| w.accept(c).is_some_and(|h| h.ok))
        };
        assert!(heard("draco-pos-heycru"), "the owner's Hey Cru wakes");
        for n in ["draco-neg-heyyou", "draco-neg-heywho", "draco-neg-heythere"] { assert!(!heard(n), "{n} must not wake"); }
    }

    /// T16: the same recordings through the microphone's path (mic): as
    /// PipeWire's ALSA "default" gives them (44.1 kHz stereo F32, in 10 ms
    /// blocks), with a laptop fan's steady noise at what farhan-RP-15's
    /// microphone gives at rest (about -34 dBFS), to mono, cleaned (Clean, as
    /// Chromium cleans Electron's), to 16 kHz by To16. The training finds the
    /// phrase (modelHeard) and the owner's Hey Cru wakes; other speech and
    /// silence do not. Without the noise too.
    #[test]
    fn wakes_through_the_microphone_path() {
        let eval = std::env::var("VW_EVAL_DIR").unwrap_or_else(|_| "/tmp/cru-eval".into());
        let models = std::env::var("VW_INSTALL_DIR").unwrap_or_else(|_| "/tmp/vw-install".into());
        if !Path::new(&models).join(MODEL_ID).exists() || !Path::new(&format!("{eval}/draco-train1.wav")).exists() { eprintln!("skip: no recordings or models"); return; }
        unsafe { std::env::set_var("CRUNDI_WAKE_MODELS", &models); }
        for noise_rms in [0.0f32, 0.02] {
            let mic = |s: Vec<f32>| -> (Vec<f32>, Vec<f32>) {
                let (rate, ch) = (44100usize, 2usize);
                let mut seed = 7u32;
                let mut lp = 0f32;
                let mut noise = move || { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); let w = (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5; lp = lp * 0.9 + w * 0.1; lp };
                let pad = |v: Vec<f32>| { let mut p = vec![0f32; RATE]; p.extend(v); p.extend(vec![0f32; RATE]); p };
                let s = pad(s);
                let n = s.len() * rate / RATE;
                let mut st = Vec::with_capacity(n * ch);
                for i in 0..n {
                    let x = i as f64 * RATE as f64 / rate as f64;
                    let (a, f) = (x as usize, (x - x.floor()) as f32);
                    let v = s.get(a).copied().unwrap_or(0.0) * (1.0 - f) + s.get(a + 1).copied().unwrap_or(0.0) * f + noise() * noise_rms / 0.053;
                    for _ in 0..ch { st.push(v); }
                }
                // V2: as the client does, the cleaned audio and its undenoised twin.
                let mut c = crundi_core::capture::Clean::for_wake(rate as u32);
                let (mut d, mut dr) = (To16::new(crundi_core::capture::OUT_RATE as usize), To16::new(crundi_core::capture::OUT_RATE as usize));
                let (mut clean, mut raw) = (vec![], vec![]);
                for b in st.chunks(441 * ch) { let (x, r) = c.push_both(&crundi_core::voice::mono(b, ch)); let o = d.push(&x); let mut q = dr.push(&r); q.resize(o.len(), 0.0); clean.extend(o); raw.extend(q); }
                (clean, raw)
            };
            let wav = |n: &str| std::fs::read(format!("{eval}/{n}.wav")).ok().and_then(|b| read_wav(&b)).map(&mic);
            let pairs: Vec<(Vec<f32>, Vec<f32>)> = (1..=5).map(|i| wav(&format!("draco-train{i}")).unwrap()).collect();
            let recs: Vec<Vec<f32>> = pairs.iter().map(|p| p.0.clone()).collect();
            let raws: Vec<Vec<f32>> = pairs.iter().map(|p| p.1.clone()).collect();
            let (profile, _) = train_with_voice_raw(&recs, Some(&raws)).unwrap_or_else(|(why, redo)| panic!("noise {noise_rms}: training: {why} {redo:?}"));
            eprintln!("noise {noise_rms}: trained: threshold {} modelHeard {} phraseFrames {} spellings {}", profile["keywordsThreshold"], profile["modelHeard"], profile["phraseFrames"], profile["spellings"]);
            assert!(profile["modelHeard"].as_f64().unwrap_or(0.0) >= 3.0, "noise {noise_rms}: the model hears the phrase in most takes");
            let heard = |s: (Vec<f32>, Vec<f32>)| -> bool { let mut w = Wake::new(&profile).unwrap(); s.0.chunks(1600).zip(s.1.chunks(1600)).any(|(c, r)| w.accept_both(c, r).is_some_and(|h| h.ok)) };
            assert!(heard(wav("draco-pos-heycru").unwrap()), "noise {noise_rms}: the owner's Hey Cru wakes through the microphone path");
            assert!(!heard(wav("draco-neg-heyyou").unwrap()), "noise {noise_rms}: other speech does not wake");
            assert!(!heard(mic(vec![0.0; RATE * 3])), "noise {noise_rms}: silence (and the fan) does not wake");
            // V2: a voice the model can't hear (the owner's: modelHeard 1):
            // the takes decide, the hum guard keeps a hum out.
            let mut own = profile.clone();
            own["modelHeard"] = serde_json::json!(0);
            eprintln!("noise {noise_rms}: consonance of the takes {}, needed {}", own["consonance"], enroll::consonance_needed(&own));
            let heard_own = |s: (Vec<f32>, Vec<f32>)| -> bool { let mut w = Wake::new(&own).unwrap(); s.0.chunks(1600).zip(s.1.chunks(1600)).any(|(c, r)| w.accept_both(c, r).is_some_and(|h| h.ok)) };
            assert!(heard_own(wav("draco-pos-heycru").unwrap()), "noise {noise_rms}: untrusted model: the owner's Hey Cru wakes");
            assert!(!heard_own(mic(vec![0.0; RATE * 3])), "noise {noise_rms}: untrusted model: silence and the fan do not wake");
            eprintln!("noise {noise_rms}: untrusted model, 'hey you' wakes: {}", heard_own(wav("draco-neg-heyyou").unwrap()));
        }
    }
}
