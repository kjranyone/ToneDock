use std::cell::Cell;
use std::ffi::c_void;
use std::path::PathBuf;

use vst3::Steinberg::Vst::{
    self, AudioBusBuffers, AudioBusBuffers__type0, IAudioProcessor, IAudioProcessorTrait,
    IComponent, IComponentTrait, IEditController, IEditControllerTrait, ProcessData, ProcessSetup,
    Sample32,
};
use vst3::Steinberg::{
    kResultOk, IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait, PClassInfo, TBool,
};
use vst3::{ComPtr, Interface};

use super::scanner::PluginInfo;
use crate::audio::chain::ParamInfo;

const SYMBOLIC_SAMPLE_SIZE_32: i32 = 0;
const PROCESS_MODE_REALTIME: i32 = 0;
const MEDIA_TYPE_AUDIO: i32 = 0;
const BUS_DIR_INPUT: i32 = 0;
const BUS_DIR_OUTPUT: i32 = 1;

pub struct LoadedPlugin {
    _library: libloading::Library,
    component: ComPtr<IComponent>,
    edit_controller: Option<ComPtr<IEditController>>,
    audio_processor: ComPtr<IAudioProcessor>,
    #[allow(dead_code)]
    info: PluginInfo,
    num_inputs: i32,
    num_outputs: i32,
    has_editor_cached: Cell<Option<bool>>,
}

impl LoadedPlugin {
    pub fn load(info: &PluginInfo) -> anyhow::Result<Self> {
        log::info!(
            "LoadedPlugin::load start: {:?} (is_dir={})",
            info.path,
            info.path.is_dir()
        );
        let dll_path = Self::find_dll(&info.path)?;
        log::info!("Found DLL: {:?}", dll_path);
        let library = unsafe {
            libloading::Library::new(&dll_path)
                .map_err(|e| anyhow::anyhow!("Failed to load library: {}", e))?
        };
        log::info!("Library loaded successfully");

        let get_factory: libloading::Symbol<unsafe extern "system" fn() -> *mut c_void> = unsafe {
            library
                .get(b"GetPluginFactory")
                .map_err(|e| anyhow::anyhow!("GetPluginFactory not found: {}", e))?
        };

        let factory_ptr = unsafe { get_factory() };
        if factory_ptr.is_null() {
            return Err(anyhow::anyhow!("Factory returned null"));
        }
        log::info!("GetPluginFactory returned non-null");

        let factory: ComPtr<IPluginFactory> = unsafe {
            ComPtr::from_raw(factory_ptr as *mut IPluginFactory)
                .ok_or_else(|| anyhow::anyhow!("Failed to create ComPtr for factory"))?
        };

        let class_count = unsafe { factory.countClasses() };
        log::info!("Factory class count: {}", class_count);
        if class_count <= 0 {
            return Err(anyhow::anyhow!("No classes found in plugin"));
        }

        let mut target_cid = None;
        for i in 0..class_count {
            let mut class_info: PClassInfo = unsafe { std::mem::zeroed() };
            let result = unsafe { factory.getClassInfo(i, &mut class_info) };
            if result == kResultOk {
                let cat =
                    unsafe { std::ffi::CStr::from_ptr(class_info.category.as_ptr() as *const i8) };
                log::info!("Class {}: category={:?}", i, cat.to_bytes());
                if cat.to_bytes() == b"Audio Module Class" {
                    target_cid = Some(class_info.cid);
                    break;
                }
            }
        }

        let cid = target_cid.ok_or_else(|| anyhow::anyhow!("No audio processor class found"))?;
        log::info!("Found audio processor class");

        let iid_component = <IComponent as Interface>::IID;
        let mut obj_ptr: *mut c_void = std::ptr::null_mut();
        let result = unsafe {
            factory.createInstance(
                cid.as_ptr() as *const _,
                iid_component.as_ptr() as *const _,
                &mut obj_ptr,
            )
        };

        if result != kResultOk || obj_ptr.is_null() {
            return Err(anyhow::anyhow!("createInstance failed"));
        }
        log::info!("createInstance succeeded");

        let component: ComPtr<IComponent> = unsafe {
            ComPtr::from_raw(obj_ptr as *mut IComponent)
                .ok_or_else(|| anyhow::anyhow!("Failed to wrap component"))?
        };

        unsafe {
            let result = component.initialize(std::ptr::null_mut());
            log::info!("initialize() returned: {}", result);
            if result != kResultOk {
                return Err(anyhow::anyhow!("initialize() failed: {}", result));
            }
        }

        let num_inputs = unsafe { component.getBusCount(MEDIA_TYPE_AUDIO, BUS_DIR_INPUT) };
        let num_outputs = unsafe { component.getBusCount(MEDIA_TYPE_AUDIO, BUS_DIR_OUTPUT) };
        log::info!("Bus counts: inputs={}, outputs={}", num_inputs, num_outputs);

        for i in 0..num_inputs.max(0) {
            unsafe {
                component.activateBus(MEDIA_TYPE_AUDIO, BUS_DIR_INPUT, i, 1 as TBool);
            }
        }
        for i in 0..num_outputs.max(0) {
            unsafe {
                component.activateBus(MEDIA_TYPE_AUDIO, BUS_DIR_OUTPUT, i, 1 as TBool);
            }
        }
        log::info!("Buses activated");

        let audio_processor: ComPtr<IAudioProcessor> = component
            .cast::<IAudioProcessor>()
            .ok_or_else(|| anyhow::anyhow!("IAudioProcessor not supported"))?;
        log::info!("IAudioProcessor interface obtained");

        let edit_controller: Option<ComPtr<IEditController>> = component.cast::<IEditController>();
        log::info!(
            "IEditController: {}",
            if edit_controller.is_some() {
                "available"
            } else {
                "not available"
            }
        );

        log::info!("LoadedPlugin::load completed successfully");
        Ok(Self {
            _library: library,
            component,
            edit_controller,
            audio_processor,
            info: info.clone(),
            num_inputs,
            num_outputs,
            has_editor_cached: Cell::new(None),
        })
    }

    pub fn setup_processing(&mut self, sample_rate: f64, block_size: i32) -> anyhow::Result<()> {
        log::info!(
            "setup_processing start: sr={}, bs={}",
            sample_rate,
            block_size
        );
        let mut setup = ProcessSetup {
            processMode: PROCESS_MODE_REALTIME,
            symbolicSampleSize: SYMBOLIC_SAMPLE_SIZE_32,
            maxSamplesPerBlock: block_size,
            sampleRate: sample_rate,
        };

        let result = unsafe { self.audio_processor.setupProcessing(&mut setup) };
        log::info!("setupProcessing returned: {}", result);
        if result != kResultOk {
            return Err(anyhow::anyhow!("setupProcessing failed: {}", result));
        }

        let in_arr = if self.num_inputs >= 2 {
            [Vst::SpeakerArr::kStereo as u64]
        } else {
            [Vst::SpeakerArr::kMono as u64]
        };
        let out_arr = [Vst::SpeakerArr::kStereo as u64; 2];

        unsafe {
            self.audio_processor.setBusArrangements(
                in_arr.as_ptr() as *mut _,
                self.num_inputs,
                out_arr.as_ptr() as *mut _,
                self.num_outputs,
            );
        }
        log::info!("setBusArrangements called");

        unsafe {
            self.component.setActive(1 as TBool);
        }
        log::info!("setActive(true) called");
        unsafe {
            self.audio_processor.setProcessing(1 as TBool);
        }
        log::info!("setProcessing(true) called, setup_processing complete");

        Ok(())
    }

    pub fn process_in_place(&mut self, buffer: &mut [&mut [f32]], num_frames: i32) {
        let num_ch = buffer.len().min(self.num_outputs.max(0) as usize);
        if num_ch == 0 {
            return;
        }

        let mut channel_ptrs: Vec<*mut Sample32> = Vec::with_capacity(num_ch);
        for ch in &mut buffer[..num_ch] {
            channel_ptrs.push(ch.as_mut_ptr() as *mut Sample32);
        }

        let io_buf = AudioBusBuffers {
            numChannels: num_ch as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: channel_ptrs.as_mut_ptr(),
            },
        };

        let mut input_buffers = io_buf;
        let mut output_buffers = io_buf;

        let mut process_data = ProcessData {
            processMode: PROCESS_MODE_REALTIME,
            symbolicSampleSize: SYMBOLIC_SAMPLE_SIZE_32,
            numSamples: num_frames,
            numInputs: 1,
            numOutputs: 1,
            inputs: &mut input_buffers,
            outputs: &mut output_buffers,
            inputParameterChanges: std::ptr::null_mut(),
            outputParameterChanges: std::ptr::null_mut(),
            inputEvents: std::ptr::null_mut(),
            outputEvents: std::ptr::null_mut(),
            processContext: std::ptr::null_mut(),
        };

        unsafe {
            self.audio_processor.process(&mut process_data);
        }
    }

    pub fn has_editor(&self) -> bool {
        if let Some(cached) = self.has_editor_cached.get() {
            return cached;
        }
        let result = if let Some(ref ec) = self.edit_controller {
            unsafe {
                let view_ptr = ec.createView(b"editor\0".as_ptr() as *const i8);
                if !view_ptr.is_null() {
                    if let Some(_view) = vst3::ComPtr::<vst3::Steinberg::IPlugView>::from_raw(
                        view_ptr as *mut vst3::Steinberg::IPlugView,
                    ) {
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        } else {
            false
        };
        self.has_editor_cached.set(Some(result));
        result
    }

    pub fn edit_controller(&self) -> Option<&ComPtr<IEditController>> {
        self.edit_controller.as_ref()
    }

    pub fn parameter_info(&self) -> Vec<ParamInfo> {
        if let Some(ref ec) = self.edit_controller {
            let count = unsafe { ec.getParameterCount() };
            let mut params = Vec::new();
            for i in 0..count {
                let mut param_info: Vst::ParameterInfo = unsafe { std::mem::zeroed() };
                unsafe {
                    ec.getParameterInfo(i, &mut param_info);
                }
                let name = {
                    let slice = &param_info.title;
                    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
                    String::from_utf16_lossy(&slice[..len])
                };
                params.push(ParamInfo {
                    id: param_info.id,
                    name,
                });
            }
            params
        } else {
            Vec::new()
        }
    }

    pub fn get_parameter(&self, index: usize) -> f32 {
        if let Some(ref ec) = self.edit_controller {
            let params = self.parameter_info();
            if let Some(p) = params.get(index) {
                unsafe { ec.getParamNormalized(p.id) as f32 }
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    pub fn set_parameter(&mut self, index: usize, value: f32) {
        if let Some(ref ec) = self.edit_controller {
            let params = self.parameter_info();
            if let Some(p) = params.get(index) {
                unsafe {
                    ec.setParamNormalized(p.id, value as f64);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn find_dll(bundle_path: &std::path::Path) -> anyhow::Result<PathBuf> {
        if bundle_path.is_file() {
            return Ok(bundle_path.to_path_buf());
        }
        let contents_dir = bundle_path.join("Contents");
        if contents_dir.is_dir() {
            if let Some(dll) = Self::find_dll_recursive(&contents_dir, 3) {
                return Ok(dll);
            }
        }
        Err(anyhow::anyhow!("No DLL found in VST3 bundle"))
    }

    #[cfg(target_os = "windows")]
    fn find_dll_recursive(dir: &std::path::Path, depth: usize) -> Option<PathBuf> {
        if depth == 0 {
            return None;
        }
        for entry in std::fs::read_dir(dir).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_file() {
                let is_binary = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("dll") || e.eq_ignore_ascii_case("vst3"))
                    .unwrap_or(false);
                if is_binary {
                    return Some(path);
                }
            } else if let Some(dll) = Self::find_dll_recursive(&path, depth - 1) {
                return Some(dll);
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    fn find_dll(bundle_path: &std::path::Path) -> anyhow::Result<PathBuf> {
        Ok(bundle_path.to_path_buf())
    }
}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        unsafe {
            let _ = self.audio_processor.setProcessing(0 as TBool);
            let _ = self.component.setActive(0 as TBool);
            let _ = self.component.terminate();
        }
    }
}
