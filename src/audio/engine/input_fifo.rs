use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub(super) struct RingBuffer {
    data: Vec<f32>,
    capacity: usize,
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0.0f32; capacity],
            capacity,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, sample: f32) {
        let w = self.write_pos.load(Ordering::Relaxed);
        let next_w = (w + 1) % self.capacity;
        let r = self.read_pos.load(Ordering::Acquire);

        if next_w == r {
            // Buffer full, drop or advance read pos (overwrite)
            let _ = self.read_pos.compare_exchange(
                r,
                (r + 1) % self.capacity,
                Ordering::Release,
                Ordering::Relaxed,
            );
        }

        // Safety: We are the only producer (input thread)
        let ptr = self.data.as_ptr() as *mut f32;
        unsafe { *ptr.add(w) = sample };
        self.write_pos.store(next_w, Ordering::Release);
    }

    pub fn pop(&self) -> Option<f32> {
        let r = self.read_pos.load(Ordering::Relaxed);
        let w = self.write_pos.load(Ordering::Acquire);

        if r == w {
            return None;
        }

        let sample = self.data[r];
        self.read_pos
            .store((r + 1) % self.capacity, Ordering::Release);
        Some(sample)
    }

    pub fn len(&self) -> usize {
        let w = self.write_pos.load(Ordering::Acquire);
        let r = self.read_pos.load(Ordering::Acquire);
        if w >= r {
            w - r
        } else {
            self.capacity - r + w
        }
    }

    pub fn skip(&self, count: usize) {
        let r = self.read_pos.load(Ordering::Relaxed);
        let w = self.write_pos.load(Ordering::Acquire);
        let available = if w >= r { w - r } else { self.capacity - r + w };
        let to_skip = count.min(available);
        self.read_pos
            .store((r + to_skip) % self.capacity, Ordering::Release);
    }
}

pub(super) struct InputFifo {
    channels: Vec<Arc<RingBuffer>>,
    max_frames: usize,
    target_frames: usize,
}

impl InputFifo {
    pub(super) fn new(max_channels: usize, max_frames: usize, target_frames: usize) -> Self {
        Self {
            channels: (0..max_channels)
                .map(|_| Arc::new(RingBuffer::new(max_frames + 1)))
                .collect(),
            max_frames,
            target_frames: target_frames.min(max_frames),
        }
    }

    fn ensure_channels(&mut self, channels: usize) {
        while self.channels.len() < channels {
            self.channels
                .push(Arc::new(RingBuffer::new(self.max_frames + 1)));
        }
    }

    pub(super) fn push_interleaved(&mut self, data: &[f32], channels: usize) {
        if channels == 0 {
            return;
        }
        self.ensure_channels(channels);
        let frames = data.len() / channels;
        for frame in 0..frames {
            for ch in 0..channels {
                self.channels[ch].push(data[frame * channels + ch]);
            }
        }
    }

    pub(super) fn pop_mono_into(&self, channel: usize, output: &mut [f32], gain: f32) {
        if self.channels.is_empty() {
            output.fill(0.0);
            return;
        }

        let src_ch = channel.min(self.channels.len() - 1);
        let rb = &self.channels[src_ch];

        let available = rb.len();
        let requested = output.len();

        // Rebalance logic: if we have way too many frames, skip them to reduce latency
        let threshold = self.target_frames + requested * 2;
        if available >= threshold {
            let to_skip = available - self.target_frames - requested;
            for ch_rb in &self.channels {
                ch_rb.skip(to_skip);
            }
        }

        for frame in 0..requested {
            output[frame] = rb.pop().unwrap_or(0.0) * gain;
        }

        // Keep other channels in sync
        for (i, ch_rb) in self.channels.iter().enumerate() {
            if i != src_ch {
                ch_rb.skip(requested);
            }
        }
    }
}
