use std::ffi::c_void;

use vst3::ComWrapper;
use vst3::Steinberg::Vst::{self, AudioBusBuffers, AudioBusBuffers__type0, ProcessSetup};
use vst3::Steinberg::{kNotImplemented, kResultFalse, kResultOk, IBStream};

use super::attributes::MemoryStream;
use super::seh_ffi::*;
use super::{
    LoadedPlugin, BUS_DIR_INPUT, BUS_DIR_OUTPUT, MEDIA_TYPE_AUDIO, PROCESS_MODE_REALTIME,
    SYMBOLIC_SAMPLE_SIZE_32,
};

impl LoadedPlugin {
    pub fn setup_processing(&mut self, sample_rate: f64, block_size: i32) -> anyhow::Result<()> {
        self.sample_rate = sample_rate;
        self.block_size = block_size;
        let in_arr = vec![Vst::SpeakerArr::kMono as u64; self.num_inputs as usize];
        let out_arr = vec![Vst::SpeakerArr::kStereo as u64; self.num_outputs as usize];

        unsafe {
            let p = self.audio_processor.as_ptr() as *mut c_void;
            let c = self.component.as_ptr() as *mut c_void;

            let res = seh_call_set_io_mode(c, 0);
            if res != kResultOk {
                log::warn!("setIoMode returned {} for '{}'", res, self.info.name);
            }

            let res = seh_call_set_bus_arrangements(
                p,
                in_arr.as_ptr() as *mut _,
                self.num_inputs,
                out_arr.as_ptr() as *mut _,
                self.num_outputs,
            );
            if res != kResultOk {
                log::warn!(
                    "setBusArrangements returned {} for '{}'",
                    res,
                    self.info.name
                );
            }

            for i in 0..self.num_inputs {
                let res = seh_call_activate_bus(c, MEDIA_TYPE_AUDIO, BUS_DIR_INPUT, i, 1);
                if res != kResultOk {
                    log::warn!(
                        "activateBus(input,{}) returned {} for '{}'",
                        i,
                        res,
                        self.info.name
                    );
                }
            }

            for i in 0..self.num_outputs {
                let res = seh_call_activate_bus(c, MEDIA_TYPE_AUDIO, BUS_DIR_OUTPUT, i, 1);
                if res != kResultOk {
                    log::warn!(
                        "activateBus(output,{}) returned {} for '{}'",
                        i,
                        res,
                        self.info.name
                    );
                }
            }

            let mut setup = ProcessSetup {
                processMode: PROCESS_MODE_REALTIME,
                symbolicSampleSize: SYMBOLIC_SAMPLE_SIZE_32,
                maxSamplesPerBlock: block_size,
                sampleRate: sample_rate,
            };
            let res = seh_call_setup_processing(p, &mut setup as *mut _ as *mut _);
            if res != kResultOk {
                return Err(anyhow::anyhow!(
                    "setupProcessing returned {} for '{}'",
                    res,
                    self.info.name
                ));
            }

            let res = seh_call_set_active(c, 1);
            if res != kResultOk {
                log::warn!("setActive(true) returned {} for '{}'", res, self.info.name);
            }
            let res = seh_call_set_processing(p, 1);
            if res != kResultOk {
                log::warn!(
                    "setProcessing(true) returned {} for '{}'",
                    res,
                    self.info.name
                );
            }
        }
        Ok(())
    }

    pub fn save_state(&self) -> Option<Vec<u8>> {
        let stream = ComWrapper::new(MemoryStream::default());
        let stream_ptr = stream.to_com_ptr::<IBStream>().unwrap();

        let get_state = unsafe {
            seh_call_component_get_state(
                self.component.as_ptr() as *mut c_void,
                stream_ptr.as_ptr() as *mut c_void,
            )
        };
        if !result_is_success(get_state) {
            if get_state != kNotImplemented && get_state != kResultFalse {
                log::warn!("getState returned {} for '{}'", get_state, self.info.name);
            }
            return None;
        }

        Some(stream.to_vec())
    }

    pub fn restore_state(&mut self, state: &[u8]) -> anyhow::Result<()> {
        let stream = ComWrapper::new(MemoryStream::from_bytes(state.to_vec()));
        let stream_ptr = stream.to_com_ptr::<IBStream>().unwrap();
        let component = self.component.as_ptr() as *mut c_void;
        let processor = self.audio_processor.as_ptr() as *mut c_void;
        let was_active = self.block_size > 0;

        if was_active {
            let _ = unsafe { seh_call_set_processing(processor, 0) };
            let _ = unsafe { seh_call_set_active(component, 0) };
        }

        let set_state =
            unsafe { seh_call_component_set_state(component, stream_ptr.as_ptr() as *mut c_void) };
        if !result_is_success(set_state) {
            return Err(anyhow::anyhow!(
                "IComponent::setState() returned {} for '{}'",
                set_state,
                self.info.name
            ));
        }

        if let Some(ref ec) = self.edit_controller {
            stream.rewind();
            let sync_result = unsafe {
                seh_call_set_component_state(
                    ec.as_ptr() as *mut c_void,
                    stream_ptr.as_ptr() as *mut c_void,
                )
            };
            if !result_is_success(sync_result)
                && sync_result != kNotImplemented
                && sync_result != kResultFalse
            {
                log::warn!(
                    "setComponentState returned {} for '{}'",
                    sync_result,
                    self.info.name
                );
            }
        }

        if was_active {
            let reactivate = unsafe { seh_call_set_active(component, 1) };
            if !result_is_success(reactivate) {
                log::warn!(
                    "setActive(true) returned {} after restore_state for '{}'",
                    reactivate,
                    self.info.name
                );
            }

            let restart = unsafe { seh_call_set_processing(processor, 1) };
            if !result_is_success(restart) {
                log::warn!(
                    "setProcessing(true) returned {} after restore_state for '{}'",
                    restart,
                    self.info.name
                );
            }
        }

        Ok(())
    }

    pub fn process_in_place(&mut self, buffer: &mut [&mut [f32]], num_frames: i32) {
        if buffer.is_empty() || num_frames == 0 {
            return;
        }

        let mut in_ptrs: Vec<*mut f32> = vec![buffer[0].as_mut_ptr()];
        let mut out_ptrs: Vec<*mut f32> = buffer
            .iter_mut()
            .take(2)
            .map(|ch| ch.as_mut_ptr())
            .collect();

        let in_bus = AudioBusBuffers {
            numChannels: in_ptrs.len() as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: in_ptrs.as_mut_ptr(),
            },
        };
        let out_bus = AudioBusBuffers {
            numChannels: out_ptrs.len() as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: out_ptrs.as_mut_ptr(),
            },
        };
        let mut in_buses = [in_bus];
        let mut out_buses = [out_bus];

        let mut ctx: vst3::Steinberg::Vst::ProcessContext = unsafe { std::mem::zeroed() };
        ctx.sampleRate = self.sample_rate;
        ctx.tempo = 120.0;
        ctx.timeSigNumerator = 4;
        ctx.timeSigDenominator = 4;
        ctx.state = (1 << 3) | (1 << 10);

        unsafe {
            let res = seh_call_process_robust(
                self.audio_processor.as_ptr() as *mut _,
                num_frames,
                1,
                in_buses.as_mut_ptr() as *mut _,
                1,
                out_buses.as_mut_ptr() as *mut _,
                &mut ctx as *mut _ as *mut _,
            );
            if res == SEH_CAUGHT {
                static mut LOGGED: bool = false;
                if !LOGGED {
                    log::error!("VST3 plugin crashed during process() - caught by SEH");
                    LOGGED = true;
                }
            }
        }
    }
}
