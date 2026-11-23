//! The microphone's audio cleaned as Chromium's getUserMedia cleans it for
//! Electron and the web (noiseSuppression, autoGainControl; T16, T18): the
//! wake word model and the server's owner-voice check were trained on audio
//! like that, so the raw signal of a laptop's microphone (fan noise, a low
//! gain) is neither heard as "Hey Cru" nor checkable as the owner's voice.
//!
//! In: mono samples at the microphone's rate. Out: 48 kHz mono, denoised by
//! RNNoise (nnnoiseless) and brought to a steady speech level.

use nnnoiseless::DenoiseState;

/// The rate it gives out.
pub const OUT_RATE: u32 = 48_000;
const FRAME: usize = DenoiseState::FRAME_SIZE;
/// Speech is brought to about -20 dBFS (RMS), with at most this much gain.
const TARGET: f32 = 0.1;
const MAX_GAIN: f32 = 10.0;
const MIN_GAIN: f32 = 0.5;

pub struct Clean {
    step: f64,
    pos: f64,
    last: f32,
    frame: Vec<f32>,
    ns: Box<DenoiseState<'static>>,
    out: Vec<f32>,
    level: f32,
    gain: f32,
    /// The first frame RNNoise gives back is its start-up; dropped.
    warm: bool,
}

impl Clean {
    pub fn new(rate: u32) -> Clean {
        Clean { step: rate as f64 / OUT_RATE as f64, pos: 0.0, last: 0.0, frame: Vec::with_capacity(FRAME), ns: DenoiseState::new(), out: vec![0.0; FRAME], level: 0.0, gain: 1.0, warm: false }
    }

    /// A block in (mono, -1..1): the cleaned samples so far, at 48 kHz.
    pub fn push(&mut self, x: &[f32]) -> Vec<f32> {
        let mut got = vec![];
        // To 48 kHz, linearly between samples: positions over [last, x...].
        let at = |i: usize, last: f32| if i == 0 { last } else { x[i - 1] };
        let len = x.len() as f64 + 1.0;
        while self.pos + 1.0 < len {
            let i = self.pos as usize;
            let f = (self.pos - i as f64) as f32;
            let (a, b) = (at(i, self.last), at(i + 1, self.last));
            self.frame.push((a + (b - a) * f) * 32768.0);
            if self.frame.len() == FRAME { self.frame_done(&mut got); }
            self.pos += self.step;
        }
        if let Some(l) = x.last() { self.last = *l; }
        self.pos -= x.len() as f64;
        got
    }

    fn frame_done(&mut self, got: &mut Vec<f32>) {
        let vad = self.ns.process_frame(&mut self.out, &self.frame);
        self.frame.clear();
        if !self.warm { self.warm = true; return; }
        let rms = (self.out.iter().map(|v| (v / 32768.0).powi(2)).sum::<f32>() / FRAME as f32).sqrt();
        // The speech level, followed while RNNoise hears a voice.
        if vad > 0.6 && rms > 1e-4 {
            self.level = if self.level == 0.0 { rms } else { self.level * 0.95 + rms * 0.05 };
            let want = (TARGET / self.level).clamp(MIN_GAIN, MAX_GAIN);
            self.gain += (want - self.gain) * 0.1;
        }
        let g = self.gain;
        got.extend(self.out.iter().map(|v| (v / 32768.0 * g).clamp(-1.0, 1.0)));
    }
}

/// A recording from the microphone, cleaned (Settings › Voice › Your voice:
/// one phrase at a time). Its own thread holds the stream; `stop` gives the
/// samples at 48 kHz. CRUNDI_TEST_MIC_WAV plays a WAV file instead, in real
/// time (voiceio's test hook).
pub struct Recorder { stop: std::sync::Arc<std::sync::atomic::AtomicBool>, got: std::sync::Arc<std::sync::Mutex<(Vec<f32>, f32)>>, done: std::sync::mpsc::Receiver<()> }

impl Recorder {
    pub fn start() -> Result<Recorder, String> {
        use std::sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc, Mutex};
        let stop = Arc::new(AtomicBool::new(false));
        let got = Arc::new(Mutex::new((Vec::<f32>::new(), 0f32)));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let (done_tx, done) = mpsc::channel::<()>();
        let (s2, g2) = (stop.clone(), got.clone());
        std::thread::spawn(move || {
            let feed = move |c: &mut Clean, b: &[f32], g: &Mutex<(Vec<f32>, f32)>| {
                let o = c.push(b);
                if o.is_empty() { return; }
                let lv = (o.iter().map(|x| x * x).sum::<f32>() / o.len() as f32).sqrt();
                if let Ok(mut g) = g.lock() { g.0.extend(o); g.1 = lv; }
            };
            if let Some(path) = std::env::var_os("CRUNDI_TEST_MIC_WAV") {
                let Some((rate, s)) = std::fs::read(&path).ok().and_then(|b| crate::voice::wav(&b)) else { let _ = ready_tx.send(Err("no microphone".into())); return };
                let _ = ready_tx.send(Ok(()));
                let mut c = Clean::new(rate);
                let block = rate as usize / 20;
                let mut i = 0;
                while !s2.load(Ordering::Relaxed) {
                    let b: Vec<f32> = if i < s.len() { s[i..(i + block).min(s.len())].to_vec() } else { vec![0.0; block] };
                    i += block;
                    feed(&mut c, &b, &g2);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let _ = done_tx.send(());
                return;
            }
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
            let setup = || -> Result<cpal::Stream, String> {
                let host = cpal::default_host();
                let dev = host.default_input_device().ok_or("No microphone found")?;
                let cfg = dev.default_input_config().map_err(|e| e.to_string())?;
                let (rate, ch) = (cfg.sample_rate().0, cfg.channels() as usize);
                let err = |e| eprintln!("[native] microphone: {e}");
                let g = g2.clone();
                let mut c = Clean::new(rate);
                let stream = match cfg.sample_format() {
                    cpal::SampleFormat::F32 => dev.build_input_stream(&cfg.into(), move |d: &[f32], _| feed(&mut c, &crate::voice::mono(d, ch), &g), err, None),
                    cpal::SampleFormat::I16 => dev.build_input_stream(&cfg.into(), move |d: &[i16], _| { let f: Vec<f32> = d.iter().map(|x| *x as f32 / 32768.0).collect(); feed(&mut c, &crate::voice::mono(&f, ch), &g) }, err, None),
                    _ => return Err("This microphone's format is not supported".into()),
                }.map_err(|e| e.to_string())?;
                stream.play().map_err(|e| e.to_string())?;
                Ok(stream)
            };
            match setup() {
                Ok(stream) => { let _ = ready_tx.send(Ok(())); while !s2.load(Ordering::Relaxed) { std::thread::sleep(std::time::Duration::from_millis(20)); } drop(stream); }
                Err(e) => { let _ = ready_tx.send(Err(e)); }
            }
            let _ = done_tx.send(());
        });
        ready_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or_else(|_| Err("The microphone did not start".into()))?;
        Ok(Recorder { stop, got, done })
    }
    /// The last block's level (RMS, 0..1).
    pub fn level(&self) -> f32 { self.got.lock().map(|g| g.1).unwrap_or(0.0) }
    /// Seconds recorded so far.
    pub fn seconds(&self) -> f64 { self.got.lock().map(|g| g.0.len() as f64 / OUT_RATE as f64).unwrap_or(0.0) }
    /// Stop: the samples (48 kHz mono).
    pub fn stop(self) -> Vec<f32> {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = self.done.recv_timeout(std::time::Duration::from_secs(2));
        self.got.lock().map(|mut g| std::mem::take(&mut g.0)).unwrap_or_default()
    }
}

/// A 16 kHz mono 16-bit WAV of 48 kHz samples (what the web's enrolment sends).
pub fn wav16(samples: &[f32]) -> Vec<u8> {
    let mut pcm = vec![];
    crate::voice::Down::new(OUT_RATE).push(samples, &mut pcm);
    let rate = crate::voice::RATE;
    let mut w = Vec::with_capacity(44 + pcm.len());
    w.extend_from_slice(b"RIFF"); w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes()); w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes()); w.extend_from_slice(&1u16.to_le_bytes()); w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes()); w.extend_from_slice(&(rate * 2).to_le_bytes()); w.extend_from_slice(&2u16.to_le_bytes()); w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data"); w.extend_from_slice(&(pcm.len() as u32).to_le_bytes()); w.extend_from_slice(&pcm);
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_48k_and_removes_steady_noise() {
        // A second of steady noise (as a fan) at 44.1 kHz: far quieter out.
        let mut seed = 1u32;
        let mut noise = || { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.07 };
        let x: Vec<f32> = (0..44100 * 2).map(|_| noise()).collect();
        let mut c = Clean::new(44100);
        let out: Vec<f32> = x.chunks(441).flat_map(|b| c.push(b)).collect();
        assert!((out.len() as i64 - 96000 + FRAME as i64 * 2).abs() <= FRAME as i64, "{}", out.len());
        let rms = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt();
        let (i, o) = (rms(&x[44100..]), rms(&out[48000..]));
        assert!(o < i / 4.0, "noise in {i}, out {o}");
    }

    #[test]
    fn wav16_is_a_16k_wav() {
        let w = wav16(&vec![0.25f32; 48000]);
        let (rate, s) = crate::voice::wav(&w).unwrap();
        assert_eq!((rate, s.len()), (16000, 16000));
        assert!((s[100] - 0.25).abs() < 0.01);
    }
}
