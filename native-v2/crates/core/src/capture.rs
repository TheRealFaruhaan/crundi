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
}
