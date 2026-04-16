use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub(super) struct RingBuffer {
    data: UnsafeCell<Vec<f32>>,
    capacity: usize,
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
}

// Safety: RingBuffer is SPSC — only the input thread calls push(),
// only the output thread calls pop(). UnsafeCell is safe because
// the two threads access disjoint regions protected by atomic indices.
unsafe impl Sync for RingBuffer {}
unsafe impl Send for RingBuffer {}

impl RingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: UnsafeCell::new(vec![0.0f32; capacity]),
            capacity,
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
        }
    }

    #[inline]
    unsafe fn data_mut(&self) -> &mut Vec<f32> {
        unsafe { &mut *self.data.get() }
    }

    #[inline]
    unsafe fn data_ref(&self) -> &Vec<f32> {
        unsafe { &*self.data.get() }
    }

    pub fn push(&self, sample: f32) {
        let w = self.write_pos.load(Ordering::Relaxed);
        let next_w = (w + 1) % self.capacity;
        let r = self.read_pos.load(Ordering::Acquire);

        if next_w == r {
            let _ = self.read_pos.compare_exchange(
                r,
                (r + 1) % self.capacity,
                Ordering::Release,
                Ordering::Relaxed,
            );
        }

        unsafe {
            self.data_mut()[w] = sample;
        }
        self.write_pos.store(next_w, Ordering::Release);
    }

    #[allow(dead_code)]
    pub fn pop(&self) -> Option<f32> {
        let r = self.read_pos.load(Ordering::Relaxed);
        let w = self.write_pos.load(Ordering::Acquire);

        if r == w {
            return None;
        }

        let sample = unsafe { self.data_ref()[r] };
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

    pub fn pop_bulk(&self, output: &mut [f32]) -> usize {
        let r = self.read_pos.load(Ordering::Relaxed);
        let w = self.write_pos.load(Ordering::Acquire);
        let available = if w >= r { w - r } else { self.capacity - r + w };
        let to_read = available.min(output.len());
        if to_read == 0 {
            return 0;
        }
        let data = unsafe { self.data_ref() };
        let first_chunk = to_read.min(self.capacity - r);
        output[..first_chunk].copy_from_slice(&data[r..r + first_chunk]);
        if first_chunk < to_read {
            let second_chunk = to_read - first_chunk;
            output[first_chunk..to_read].copy_from_slice(&data[..second_chunk]);
        }
        self.read_pos
            .store((r + to_read) % self.capacity, Ordering::Release);
        to_read
    }
}

pub(super) struct InputFifo {
    channels: Vec<Arc<RingBuffer>>,
    max_frames: usize,
    target_frames: usize,
    last_sample: std::sync::atomic::AtomicU32,
}

impl InputFifo {
    pub(super) fn new(max_channels: usize, max_frames: usize, target_frames: usize) -> Self {
        Self {
            channels: (0..max_channels)
                .map(|_| Arc::new(RingBuffer::new(max_frames + 1)))
                .collect(),
            max_frames,
            target_frames: target_frames.min(max_frames),
            last_sample: std::sync::atomic::AtomicU32::new(0.0f32.to_bits()),
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

        let threshold = self.target_frames + requested * 2;
        if available >= threshold {
            let to_skip = available - self.target_frames - requested;
            for ch_rb in &self.channels {
                ch_rb.skip(to_skip);
            }
        }

        let popped = rb.pop_bulk(&mut output[..]);
        let last = output[..popped].last().copied().unwrap_or(0.0);
        self.last_sample
            .store(last.to_bits(), std::sync::atomic::Ordering::Relaxed);

        if popped < requested {
            let fade_len = requested - popped;
            let fade_start = popped;
            for i in 0..fade_len {
                let fade = 1.0 - (i as f32 / fade_len.max(1) as f32).min(1.0);
                output[fade_start + i] = last * fade;
            }
        }

        if gain != 1.0 {
            for s in output.iter_mut() {
                *s *= gain;
            }
        }

        for (i, ch_rb) in self.channels.iter().enumerate() {
            if i != src_ch {
                ch_rb.skip(requested);
            }
        }
    }
}
