use std::fs::File;
use std::path::Path;
use std::sync::atomic::Ordering;

use symphonia::core::audio::Signal;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::audio::graph::BackingTrackBuffer;
use crate::audio::node::{
    BackingTrackNodeState, Connection, NodeId, NodeInternalState, NodeType, PortId,
};

use super::helpers::commit_and_publish_graph;
use super::AudioEngine;

pub(super) fn decode_audio_file(path: &Path) -> anyhow::Result<(Vec<Vec<f32>>, f64)> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let metadata_opts = MetadataOptions::default();
    let decoder_opts = DecoderOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| anyhow::anyhow!("Failed to probe audio file: {}", e))?;

    let mut format_reader = probed.format;
    let track = format_reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow::anyhow!("No supported audio track found"))?;

    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100) as f64;
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .map_err(|e| anyhow::anyhow!("Failed to create decoder: {}", e))?;

    let mut channel_data: Vec<Vec<f32>> = vec![Vec::new(); channels];

    loop {
        let packet = match format_reader.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => {
                log::debug!("Decoder ended: {}", e);
                break;
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                append_decoded_samples(&mut channel_data, &decoded, channels);
            }
            Err(e) => {
                log::warn!("Decode error: {}", e);
                continue;
            }
        }
    }

    let total_frames = channel_data.first().map(|c| c.len()).unwrap_or(0);
    if total_frames == 0 {
        return Err(anyhow::anyhow!("No audio data decoded"));
    }

    Ok((channel_data, sample_rate))
}

fn append_decoded_samples(
    channel_data: &mut [Vec<f32>],
    decoded: &symphonia::core::audio::AudioBufferRef,
    channels: usize,
) {
    use symphonia::core::audio::AudioBufferRef;
    let num_frames = decoded.frames();

    let write_samples = |channel_data: &mut [Vec<f32>],
                         ch_count: usize,
                         get_sample: &dyn Fn(usize, usize) -> f32| {
        for frame in 0..num_frames {
            for ch in 0..channels.min(ch_count) {
                if ch < channel_data.len() {
                    channel_data[ch].push(get_sample(ch, frame));
                }
            }
        }
    };

    match decoded {
        AudioBufferRef::U8(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| {
                buf.chan(ch)[frame] as f32 / 128.0 - 1.0
            });
        }
        AudioBufferRef::U16(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| {
                buf.chan(ch)[frame] as f32 / 32768.0 - 1.0
            });
        }
        AudioBufferRef::U24(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| {
                buf.chan(ch)[frame].0 as f32 / 8388608.0 - 1.0
            });
        }
        AudioBufferRef::U32(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| {
                buf.chan(ch)[frame] as f32 / 2147483648.0 - 1.0
            });
        }
        AudioBufferRef::S8(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| {
                buf.chan(ch)[frame] as f32 / 128.0
            });
        }
        AudioBufferRef::S16(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| {
                buf.chan(ch)[frame] as f32 / 32768.0
            });
        }
        AudioBufferRef::S24(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| {
                buf.chan(ch)[frame].0 as f32 / 8388608.0
            });
        }
        AudioBufferRef::S32(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| {
                buf.chan(ch)[frame] as f32 / 2147483648.0
            });
        }
        AudioBufferRef::F32(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| buf.chan(ch)[frame]);
        }
        AudioBufferRef::F64(buf) => {
            let count = buf.spec().channels.count();
            write_samples(channel_data, count, &|ch, frame| buf.chan(ch)[frame] as f32);
        }
    }
}

impl AudioEngine {
    pub fn load_backing_track_file(&self, node_id: NodeId, path: &Path) -> anyhow::Result<()> {
        let (channel_data, file_sample_rate) = decode_audio_file(path)?;
        let frames = channel_data.first().map(|c| c.len()).unwrap_or(0);
        let channels = channel_data.len();
        log::info!(
            "Loaded backing track: {:?} ({} ch x {} frames @ {} Hz)",
            path,
            channels,
            frames,
            file_sample_rate
        );

        let sample_rate = self.sample_rate;
        let resampled = if (file_sample_rate - sample_rate).abs() > 1.0 {
            log::debug!(
                "load_backing_track: resampling {} -> {} Hz",
                file_sample_rate,
                sample_rate
            );
            resample(&channel_data, file_sample_rate, sample_rate)
        } else {
            channel_data
        };
        let buffer = BackingTrackBuffer::new(resampled, sample_rate);

        {
            let guard = self.graph.load();
            let mut staging = (**guard).clone();
            drop(guard);
            staging.set_backing_track_buffer(node_id, buffer);
            if let Some(node) = staging.get_node_mut(node_id) {
                node.internal_state = NodeInternalState::BackingTrack(BackingTrackNodeState {
                    playing: false,
                    volume: 1.0,
                    speed: 1.0,
                    looping: true,
                    file_loaded: true,
                    loop_start: None,
                    loop_end: None,
                    pitch_semitones: 0.0,
                    pre_roll_secs: 0.0,
                    section_markers: vec![],
                });
            }
            commit_and_publish_graph(&self.graph, staging, None)?;
        }

        Ok(())
    }

    pub fn add_backing_track_node(&mut self) -> NodeId {
        if let Some(id) = self.backing_track_node_id {
            return id;
        }
        self.graph_add_node(NodeType::BackingTrack);
        self.apply_commands_to_staging();
        let id = self.find_node_id_by_type(NodeType::BackingTrack);
        if let Some(id) = id {
            self.backing_track_node_id = Some(id);
            return id;
        }
        NodeId(0)
    }

    pub fn ensure_backing_track_in_graph(&mut self) -> NodeId {
        if let Some(id) = self.backing_track_node_id {
            return id;
        }
        let id = self.add_backing_track_node();

        let mixer_id = self.master_mixer_id;
        let (already_connected, current_inputs) = {
            let guard = self.graph.load();
            let conns = guard.connections();
            let connected = conns
                .iter()
                .any(|c| c.source_node == id && c.target_node == mixer_id);
            let count = conns.iter().filter(|c| c.target_node == mixer_id).count();
            (connected, count)
        };

        if !already_connected {
            self.graph_connect(Connection {
                source_node: id,
                source_port: PortId(0),
                target_node: mixer_id,
                target_port: PortId(current_inputs as u32),
            });

            {
                let staging = {
                    let guard = self.graph.load();
                    (**guard).clone()
                };
                let mut staging = staging;
                if let Some(mixer) = staging.get_node_mut(mixer_id) {
                    if let NodeType::Mixer { ref mut inputs } = mixer.node_type {
                        *inputs = (*inputs as usize + 1).max(current_inputs + 1) as u16;
                    }
                }
                staging.set_node_position(id, 100.0, 450.0);
                if let Err(e) = commit_and_publish_graph(&self.graph, staging, None) {
                    log::error!("Failed to connect backing track: {}", e);
                }
            }
        }

        id
    }

    pub fn backing_track_duration(&self, node_id: NodeId) -> f64 {
        let guard = self.graph.load();
        guard.backing_track_duration_secs(node_id)
    }

    pub fn backing_track_position(&self, node_id: NodeId) -> f64 {
        let guard = self.graph.load();
        guard.backing_track_position_secs(node_id)
    }

    pub fn backing_track_seek(&self, node_id: NodeId, position_secs: f64) {
        let guard = self.graph.load();
        let staging = (**guard).clone();
        drop(guard);
        staging.backing_track_seek(node_id, position_secs);
        if let Err(e) = commit_and_publish_graph(&self.graph, staging, None) {
            log::error!("Backing track seek failed: {}", e);
        }
    }

    #[allow(dead_code)]
    pub fn export_looper_wav(&self, node_id: NodeId, path: &Path) -> anyhow::Result<()> {
        let guard = self.graph.load();
        let samples = guard
            .export_looper_samples(node_id)
            .ok_or_else(|| anyhow::anyhow!("No looper data to export"))?;
        drop(guard);

        let channels = samples.len();
        let num_frames = samples.first().map(|c| c.len()).unwrap_or(0);
        if num_frames == 0 {
            return Err(anyhow::anyhow!("Looper buffer is empty"));
        }

        let spec = hound::WavSpec {
            channels: channels as u16,
            sample_rate: self.sample_rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(path, spec)?;
        for frame in 0..num_frames {
            for ch in 0..channels {
                let sample = samples[ch].get(frame).copied().unwrap_or(0.0);
                writer.write_sample(sample)?;
            }
        }
        writer.finalize()?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn import_looper_wav(&self, node_id: NodeId, path: &Path) -> anyhow::Result<()> {
        let mut reader = hound::WavReader::open(path)?;
        let spec = reader.spec();
        let channels = spec.channels as usize;

        let mut samples: Vec<Vec<f32>> = vec![Vec::new(); channels];

        match spec.sample_format {
            hound::SampleFormat::Float => {
                let mut idx = 0;
                for result in reader.samples::<f32>() {
                    let sample = result?;
                    samples[idx % channels].push(sample);
                    idx += 1;
                }
            }
            hound::SampleFormat::Int => {
                let max_val = (1i64 << (spec.bits_per_sample as i32 - 1)) as f32;
                let mut idx = 0;
                for result in reader.samples::<i32>() {
                    let sample = result? as f32 / max_val;
                    samples[idx % channels].push(sample);
                    idx += 1;
                }
            }
        }

        let actual_frames = samples.first().map(|c| c.len()).unwrap_or(0);
        let guard = self.graph.load();
        guard.import_looper_samples(node_id, &samples, actual_frames);
        drop(guard);

        Ok(())
    }

    pub fn start_recorder(&self, node_id: NodeId, channels: usize) {
        let guard = self.graph.load();
        let mut staging = (**guard).clone();
        drop(guard);
        if let Some(node) = staging.get_node_mut(node_id) {
            node.internal_state =
                NodeInternalState::Recorder(crate::audio::node::RecorderNodeState {
                    recording: true,
                    has_data: false,
                });
            node.buffers.get_mut().recorder_buffer = Some(vec![Vec::new(); channels]);
        }
        let _ = commit_and_publish_graph(&self.graph, staging, None);
    }

    pub fn stop_recorder(&self, node_id: NodeId) {
        let guard = self.graph.load();
        // Read the recorded data from the runtime view — the audio thread
        // writes recorder_buffer there.
        let has_data = guard.get_node_runtime(node_id).map_or(false, |n| {
            n.buffers().recorder_buffer.as_ref().map_or(false, |b| {
                b.iter().any(|c: &Vec<f32>| !c.is_empty())
            })
        });
        let mut staging = (**guard).clone();
        drop(guard);
        if let Some(node) = staging.get_node_mut(node_id) {
            node.internal_state =
                NodeInternalState::Recorder(crate::audio::node::RecorderNodeState {
                    recording: false,
                    has_data,
                });
        }
        let _ = commit_and_publish_graph(&self.graph, staging, None);
    }

    pub fn export_recorder_wav(
        &self,
        node_id: NodeId,
        path: &std::path::Path,
    ) -> anyhow::Result<()> {
        let guard = self.graph.load();
        let Some(node) = guard.get_node_runtime(node_id) else {
            return Err(anyhow::anyhow!("Recorder node not found"));
        };
        let buf = node.buffers();
        let Some(ref data) = buf.recorder_buffer else {
            return Err(anyhow::anyhow!("No recorded data"));
        };
        if data.is_empty() || data[0].is_empty() {
            return Err(anyhow::anyhow!("No recorded data"));
        }
        let channels = data.len() as u16;
        let sample_count = data[0].len() as u32;
        let spec = hound::WavSpec {
            channels,
            sample_rate: self.sample_rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)?;
        for i in 0..sample_count as usize {
            for ch in 0..channels as usize {
                let sample = data
                    .get(ch)
                    .and_then(|c: &Vec<f32>| c.get(i))
                    .copied()
                    .unwrap_or(0.0);
                writer.write_sample(sample)?;
            }
        }
        writer.finalize()?;
        drop(guard);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn recorder_has_data(&self, node_id: NodeId) -> bool {
        let guard = self.graph.load();
        guard.get_node_runtime(node_id).map_or(false, |node| {
            node.atomic_recorder_has_data.load(Ordering::Relaxed)
        })
    }

    #[allow(dead_code)]
    pub fn add_drum_machine_node(&mut self) -> NodeId {
        self.graph_add_node(NodeType::DrumMachine);
        let drum_id = self.find_node_id_by_type(NodeType::DrumMachine);
        if let Some(_id) = drum_id {
            let guard = self.graph.load();
            let mut staging = (**guard).clone();
            drop(guard);
            let new_inputs = {
                let guard2 = self.graph.load();
                guard2.get_node(self.master_mixer_id).and_then(|mixer| {
                    if let NodeType::Mixer { inputs, .. } = mixer.node_type {
                        Some(inputs + 1)
                    } else {
                        None
                    }
                })
            };
            if let Some(new_inputs) = new_inputs {
                if let Some(mixer_node) = staging.get_node_mut(self.master_mixer_id) {
                    mixer_node.node_type = NodeType::Mixer { inputs: new_inputs };
                }
            }
            let _ = commit_and_publish_graph(&self.graph, staging, None);
        }
        drum_id.unwrap_or(NodeId(0))
    }

    pub fn add_recorder_node(&mut self) -> NodeId {
        self.graph_add_node(NodeType::Recorder);
        self.find_node_id_by_type(NodeType::Recorder)
            .unwrap_or(NodeId(0))
    }
}

fn resample(data: &[Vec<f32>], from_rate: f64, to_rate: f64) -> Vec<Vec<f32>> {
    if from_rate <= 0.0 || to_rate <= 0.0 || data.is_empty() {
        return data.to_vec();
    }

    let ratio = from_rate / to_rate;
    let src_frames = data[0].len();
    let dst_frames = ((src_frames as f64) / ratio).ceil() as usize;
    let channels = data.len();

    let mut result: Vec<Vec<f32>> = vec![Vec::with_capacity(dst_frames); channels];

    for i in 0..dst_frames {
        let src_pos = i as f64 * ratio;
        let s0 = src_pos.floor() as usize;
        let s1 = (s0 + 1).min(src_frames - 1);
        let frac = src_pos - s0 as f64;

        for ch in 0..channels {
            let v0 = data[ch].get(s0).copied().unwrap_or(0.0);
            let v1 = data[ch].get(s1).copied().unwrap_or(0.0);
            result[ch].push(v0 + (v1 - v0) * frac as f32);
        }
    }

    result
}
