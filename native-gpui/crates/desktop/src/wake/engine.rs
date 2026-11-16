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
const VOICE_SHA: &str = "c59158379255ad66e161679cca6af8d52d51e389e3224ab7d7a7baae295c2db5";
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
}

impl Wake {
    pub fn new(profile: &Value) -> Result<Wake, String> {
        if !enroll::valid_profile(profile) { return Err("Hey Cru is not trained".into()); }
        let spot = Spotter::new(&enroll::listen_lines(profile), profile["keywordsScore"].as_f64().unwrap_or(1.5) as f32, profile["keywordsThreshold"].as_f64().unwrap_or(0.2) as f32, true)?;
        Ok(Wake { tpl: enroll::templates_of(profile), profile: profile.clone(), spot, seg: enroll::Segmenter::new(), loose: None, ring: vec![0.0; (VERIFY_SECONDS * RATE as f32) as usize], filled: 0, at: 0, last_hit: None })
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
    pub fn accept(&mut self, samples: &[f32]) -> Option<Heard> {
        self.push(samples);
        let k = self.spot.accept(samples);
        let burst = self.seg.accept(samples);
        if self.last_hit.is_some_and(|t| t.elapsed().as_millis() as u64 <= COOLDOWN_MS) { return None; }
        if let Some(b) = burst {
            let (profile, tpl) = (self.profile.clone(), std::mem::take(&mut self.tpl));
            let loose = &mut self.loose;
            let mut confirm = |clip: &[f32]| -> Vec<String> {
                if loose.is_none() { *loose = Spotter::new(&enroll::confirm_lines(&profile), profile["keywordsScore"].as_f64().unwrap_or(1.5) as f32, enroll::CONFIRM_THRESHOLD, false).ok(); }
                match loose.as_mut() { Some(s) => enroll::spotted(s, clip), None => vec![] }
            };
            let (ok, score, _) = enroll::match_phrase(&profile, &tpl, &b, Some(&mut confirm));
            self.tpl = tpl;
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
    pub fn reset(&mut self) { self.spot.reset(); self.seg.reset(); self.filled = 0; self.at = 0; }
}

/// Train from five recordings, with the owner's voice when its model is there.
pub fn train_with_voice(recordings: &[Vec<f32>]) -> Result<(Value, String), (String, Vec<usize>)> {
    let mut make = maker();
    match enroll::train(recordings, &mut make) {
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
}
