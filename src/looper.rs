use parking_lot::Mutex;
use std::sync::Arc;

#[allow(dead_code)]
pub struct Looper {
    pub enabled: bool,
    pub recording: bool,
    pub playing: bool,
    pub overdubbing: bool,
    num_channels: usize,
    buffer: Arc<Mutex<LooperBuffer>>,
    playback_pos: Arc<Mutex<usize>>,
}

struct LooperBuffer {
    data: Vec<Vec<f32>>,
    channels: usize,
    capacity: usize,
    write_pos: usize,
    len: usize,
}

impl LooperBuffer {
    fn new(channels: usize, sample_rate: f64, max_seconds: f64) -> Self {
        let capacity = (sample_rate * max_seconds) as usize;
        let data = vec![vec![0.0f32; capacity]; channels];
        Self {
            data,
            channels,
            capacity,
            write_pos: 0,
            len: 0,
        }
    }

    fn record_frame(&mut self, input: &[&[f32]], num_frames: usize) {
        for frame in 0..num_frames {
            for ch in 0..self.channels.min(input.len()) {
                if self.write_pos < self.capacity {
                    self.data[ch][self.write_pos] = input[ch].get(frame).copied().unwrap_or(0.0);
                }
            }
            self.write_pos = (self.write_pos + 1) % self.capacity;
            if self.len < self.capacity {
                self.len += 1;
            }
        }
    }

    fn overdub_frame(&mut self, input: &[&[f32]], num_frames: usize, pos: usize) {
        for frame in 0..num_frames {
            let p = (pos + frame) % self.len;
            for ch in 0..self.channels.min(input.len()) {
                let inp = input[ch].get(frame).copied().unwrap_or(0.0);
                self.data[ch][p] += inp;
            }
        }
    }

    fn read_frame(&self, output: &mut [&mut [f32]], num_frames: usize, pos: usize) {
        if self.len == 0 {
            return;
        }
        for frame in 0..num_frames {
            let p = (pos + frame) % self.len;
            for ch in 0..self.channels.min(output.len()) {
                if frame < output[ch].len() {
                    output[ch][frame] += self.data[ch][p];
                }
            }
        }
    }

    #[allow(dead_code)]
    fn clear(&mut self) {
        for ch in &mut self.data {
            ch.fill(0.0);
        }
        self.write_pos = 0;
        self.len = 0;
    }
}

#[allow(dead_code)]
impl Looper {
    pub fn new(channels: usize, sample_rate: f64) -> Self {
        Self {
            enabled: false,
            recording: false,
            playing: false,
            overdubbing: false,
            buffer: Arc::new(Mutex::new(LooperBuffer::new(channels, sample_rate, 120.0))),
            playback_pos: Arc::new(Mutex::new(0)),
            num_channels: channels,
        }
    }

    fn channels(&self) -> usize {
        self.num_channels
    }

    pub fn toggle_record(&mut self) {
        if self.recording {
            self.recording = false;
            self.playing = true;
        } else {
            {
                let mut buf = self.buffer.lock();
                buf.clear();
            }
            self.recording = true;
            self.playing = false;
            self.overdubbing = false;
            *self.playback_pos.lock() = 0;
        }
    }

    pub fn toggle_play(&mut self) {
        if self.playing {
            self.playing = false;
            *self.playback_pos.lock() = 0;
        } else {
            self.playing = true;
            self.recording = false;
        }
    }

    pub fn toggle_overdub(&mut self) {
        if self.overdubbing {
            self.overdubbing = false;
        } else if self.playing {
            self.overdubbing = true;
        }
    }

    pub fn clear(&mut self) {
        self.recording = false;
        self.playing = false;
        self.overdubbing = false;
        self.buffer.lock().clear();
        *self.playback_pos.lock() = 0;
    }

    pub fn process(&mut self, io: &mut [&mut [f32]], num_frames: usize) {
        if !self.enabled {
            return;
        }

        let current_len;

        {
            let buffer = self.buffer.lock();
            current_len = buffer.len;
        }

        if self.recording {
            let mut buffer = self.buffer.lock();
            let mut temp_input: Vec<Vec<f32>> = vec![vec![0.0f32; num_frames]; self.channels()];
            for ch in 0..self.channels().min(io.len()) {
                for frame in 0..num_frames {
                    temp_input[ch][frame] = io[ch].get(frame).copied().unwrap_or(0.0);
                }
            }
            let input_refs: Vec<&[f32]> = temp_input.iter().map(|v| &v[..]).collect();
            buffer.record_frame(&input_refs, num_frames);
        }

        if self.playing && current_len > 0 {
            let p = *self.playback_pos.lock();
            {
                let buffer = self.buffer.lock();
                let mut output_refs: Vec<&mut [f32]> =
                    io.iter_mut().map(|ch| &mut ch[..]).collect();
                buffer.read_frame(&mut output_refs, num_frames, p);
            }
            *self.playback_pos.lock() = (p + num_frames) % current_len;
        }

        if self.overdubbing && current_len > 0 {
            let p = *self.playback_pos.lock();
            let mut buffer = self.buffer.lock();
            let mut temp_input: Vec<Vec<f32>> = vec![vec![0.0f32; num_frames]; self.channels()];
            for ch in 0..self.channels().min(io.len()) {
                for frame in 0..num_frames {
                    temp_input[ch][frame] = io[ch].get(frame).copied().unwrap_or(0.0);
                }
            }
            let input_refs: Vec<&[f32]> = temp_input.iter().map(|v| &v[..]).collect();
            buffer.overdub_frame(&input_refs, num_frames, p);
        }
    }

    pub fn loop_length_samples(&self) -> usize {
        self.buffer.lock().len
    }
}
