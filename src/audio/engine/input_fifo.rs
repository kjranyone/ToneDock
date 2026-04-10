use std::collections::VecDeque;

pub(super) struct InputFifo {
    channels: Vec<VecDeque<f32>>,
    max_frames: usize,
    target_frames: usize,
}

impl InputFifo {
    pub(super) fn new(max_channels: usize, max_frames: usize, target_frames: usize) -> Self {
        Self {
            channels: (0..max_channels)
                .map(|_| VecDeque::with_capacity(max_frames))
                .collect(),
            max_frames,
            target_frames: target_frames.min(max_frames),
        }
    }

    fn ensure_channels(&mut self, channels: usize) {
        if self.channels.len() >= channels {
            return;
        }
        self.channels.extend(
            (self.channels.len()..channels).map(|_| VecDeque::with_capacity(self.max_frames)),
        );
    }

    fn available_frames(&self) -> usize {
        self.channels.iter().map(VecDeque::len).min().unwrap_or(0)
    }

    fn trim_to_capacity(&mut self) {
        let overflow = self.available_frames().saturating_sub(self.max_frames);
        if overflow == 0 {
            return;
        }

        for channel in &mut self.channels {
            for _ in 0..overflow.min(channel.len()) {
                let _ = channel.pop_front();
            }
        }
    }

    fn rebalance_for_output(&mut self, requested_frames: usize) {
        let available = self.available_frames();
        let keep = self.target_frames.saturating_add(requested_frames);
        let overflow = available.saturating_sub(keep);
        if overflow == 0 {
            return;
        }

        for channel in &mut self.channels {
            for _ in 0..overflow.min(channel.len()) {
                let _ = channel.pop_front();
            }
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
                self.channels[ch].push_back(data[frame * channels + ch]);
            }
        }
        self.trim_to_capacity();
    }

    pub(super) fn pop_mono_into(&mut self, channel: usize, output: &mut [f32], gain: f32) {
        output.fill(0.0);
        if self.channels.is_empty() {
            return;
        }

        self.rebalance_for_output(output.len());
        let frames = self.available_frames().min(output.len());
        if frames == 0 {
            return;
        }

        let src_ch = channel.min(self.channels.len().saturating_sub(1));
        for frame in 0..frames {
            if let Some(sample) = self.channels[src_ch].pop_front() {
                output[frame] = sample * gain;
            }
        }

        for (ch, queue) in self.channels.iter_mut().enumerate() {
            if ch == src_ch {
                continue;
            }
            for _ in 0..frames.min(queue.len()) {
                let _ = queue.pop_front();
            }
        }
    }
}
