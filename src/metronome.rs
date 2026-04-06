#[allow(dead_code)]
pub struct Metronome {
    pub bpm: f64,
    pub enabled: bool,
    pub volume: f32,
    sample_rate: f64,
    samples_per_beat: f64,
    phase: f64,
    click_remaining: usize,
}

const CLICK_FREQ: f64 = 1000.0;
const CLICK_DURATION_SAMPLES: usize = 480;

#[allow(dead_code)]
impl Metronome {
    pub fn new(sample_rate: f64) -> Self {
        let bpm = 120.0;
        let samples_per_beat = sample_rate * 60.0 / bpm;
        Self {
            bpm,
            enabled: false,
            volume: 0.5,
            sample_rate,
            samples_per_beat,
            phase: 0.0,
            click_remaining: 0,
        }
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm;
        self.samples_per_beat = self.sample_rate * 60.0 / bpm;
    }

    pub fn set_sample_rate(&mut self, sr: f64) {
        self.sample_rate = sr;
        self.samples_per_beat = sr * 60.0 / self.bpm;
    }

    pub fn process(&mut self, output: &mut [&mut [f32]], num_frames: usize) {
        if !self.enabled {
            return;
        }

        for frame in 0..num_frames {
            let sample = if self.click_remaining > 0 {
                let t = (CLICK_DURATION_SAMPLES - self.click_remaining) as f64;
                let val = (2.0 * std::f64::consts::PI * CLICK_FREQ * t / self.sample_rate).sin()
                    * (self.click_remaining as f64 / CLICK_DURATION_SAMPLES as f64);
                self.click_remaining -= 1;
                val as f32 * self.volume
            } else {
                0.0
            };

            for ch in output.iter_mut() {
                if frame < ch.len() {
                    ch[frame] += sample;
                }
            }

            self.phase += 1.0;
            if self.phase >= self.samples_per_beat {
                self.phase -= self.samples_per_beat;
                self.click_remaining = CLICK_DURATION_SAMPLES;
            }
        }
    }
}
