use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use vst3::Steinberg::Vst::{
    self, AudioBusBuffers, AudioBusBuffers__type0, IAudioProcessor, IComponent, IComponentHandler,
    IComponentTrait, IEditController, IEditControllerTrait, IHostApplication, ProcessSetup,
};
use vst3::Steinberg::{
    kResultOk, tresult, uint32, FUnknown, IPluginBaseTrait, IPluginFactory, IPluginFactoryTrait,
    PClassInfo, TBool, TUID,
};
use vst3::{ComPtr, Interface};

use super::scanner::PluginInfo;
use crate::audio::chain::ParamInfo;

const SYMBOLIC_SAMPLE_SIZE_32: i32 = 0;
const PROCESS_MODE_REALTIME: i32 = 0;
const MEDIA_TYPE_AUDIO: i32 = 0;
const BUS_DIR_INPUT: i32 = 0;
const BUS_DIR_OUTPUT: i32 = 1;

#[repr(C)]
struct HostComponentHandlerVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    begin_edit: unsafe extern "system" fn(*mut c_void, u32) -> tresult,
    perform_edit: unsafe extern "system" fn(*mut c_void, u32, f64) -> tresult,
    end_edit: unsafe extern "system" fn(*mut c_void, u32) -> tresult,
    restart_component: unsafe extern "system" fn(*mut c_void, i32) -> tresult,
}

#[repr(C)]
struct HostComponentHandler {
    vtbl: *const HostComponentHandlerVtbl,
    ref_count: AtomicUsize,
}
static HOST_COMPONENT_HANDLER_VTBL: HostComponentHandlerVtbl = HostComponentHandlerVtbl {
    query_interface: handler_qi,
    add_ref: handler_add_ref,
    release: handler_release,
    begin_edit: handler_noop2,
    perform_edit: handler_noop3,
    end_edit: handler_noop2,
    restart_component: handler_noop2i,
};
unsafe extern "system" fn handler_noop2(_: *mut c_void, _: u32) -> tresult {
    kResultOk
}
unsafe extern "system" fn handler_noop3(_: *mut c_void, _: u32, _: f64) -> tresult {
    kResultOk
}
unsafe extern "system" fn handler_noop2i(_: *mut c_void, _: i32) -> tresult {
    kResultOk
}
unsafe extern "system" fn handler_qi(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let requested = unsafe { &*(iid as *const [u8; 16]) };
    if *requested == <FUnknown as Interface>::IID
        || *requested == <IComponentHandler as Interface>::IID
    {
        unsafe {
            handler_add_ref(this);
            obj.write(this);
        }
        return kResultOk;
    }
    -1
}
unsafe extern "system" fn handler_add_ref(this: *mut c_void) -> uint32 {
    let obj = this as *const HostComponentHandler;
    unsafe { (*obj).ref_count.fetch_add(1, Ordering::Relaxed) as u32 + 1 }
}
unsafe extern "system" fn handler_release(this: *mut c_void) -> uint32 {
    let obj = this as *const HostComponentHandler;
    let count = unsafe { (*obj).ref_count.fetch_sub(1, Ordering::Relaxed) };
    if count == 1 {
        let _ = unsafe { Box::from_raw(this as *mut HostComponentHandler) };
    }
    count.saturating_sub(1) as u32
}

#[repr(C)]
struct HostApplicationVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    get_name: unsafe extern "system" fn(*mut c_void, *mut u16) -> tresult,
    create_instance: unsafe extern "system" fn(
        *mut c_void,
        *const TUID,
        *const TUID,
        *mut *mut c_void,
    ) -> tresult,
}
#[repr(C)]
struct HostApplication {
    vtbl: *const HostApplicationVtbl,
    ref_count: AtomicUsize,
}
static HOST_APPLICATION_VTBL: HostApplicationVtbl = HostApplicationVtbl {
    query_interface: host_app_qi,
    add_ref: host_app_add_ref,
    release: host_app_release,
    get_name: host_app_get_name,
    create_instance: host_app_create_instance,
};
unsafe extern "system" fn host_app_qi(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    let requested = unsafe { &*(iid as *const [u8; 16]) };
    if *requested == <FUnknown as Interface>::IID
        || *requested == <IHostApplication as Interface>::IID
    {
        unsafe {
            host_app_add_ref(this);
            obj.write(this);
        }
        return kResultOk;
    }
    -1
}
unsafe extern "system" fn host_app_add_ref(this: *mut c_void) -> uint32 {
    let obj = this as *const HostApplication;
    unsafe { (*obj).ref_count.fetch_add(1, Ordering::Relaxed) as u32 + 1 }
}
unsafe extern "system" fn host_app_release(this: *mut c_void) -> uint32 {
    let obj = this as *const HostApplication;
    let count = unsafe { (*obj).ref_count.fetch_sub(1, Ordering::Relaxed) };
    if count == 1 {
        let _ = unsafe { Box::from_raw(this as *mut HostApplication) };
    }
    count.saturating_sub(1) as u32
}
unsafe extern "system" fn host_app_get_name(_: *mut c_void, name: *mut u16) -> tresult {
    let app_name = "ToneDock\0";
    let wide: Vec<u16> = app_name.encode_utf16().collect();
    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), name, wide.len());
    }
    kResultOk
}
unsafe extern "system" fn host_app_create_instance(
    _: *mut c_void,
    _: *const TUID,
    _: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    unsafe { obj.write(std::ptr::null_mut()) };
    -1
}

pub struct LoadedPlugin {
    component: ComPtr<IComponent>,
    edit_controller: Option<ComPtr<IEditController>>,
    audio_processor: ComPtr<IAudioProcessor>,
    pub info: PluginInfo,
    pub num_inputs: i32,
    pub num_outputs: i32,
    host_application: *mut c_void,
    component_handler: *mut c_void,
    sample_rate: f64,
    block_size: i32,
    _library: libloading::Library,
}

unsafe impl Send for LoadedPlugin {}

unsafe extern "C" {
    #[link_name = "seh_call_query_interface"]
    fn seh_call_query_interface(
        com_obj: *mut c_void,
        iid: *const c_void,
        obj: *mut *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_count_classes"]
    fn seh_call_count_classes(com_obj: *mut c_void) -> i32;
    #[link_name = "seh_call_get_class_info"]
    fn seh_call_get_class_info(com_obj: *mut c_void, idx: i32, info: *mut c_void) -> i32;
    #[link_name = "seh_call_create_instance"]
    fn seh_call_create_instance(
        com_obj: *mut c_void,
        cid: *const c_void,
        iid: *const c_void,
        obj: *mut *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_initialize"]
    fn seh_call_initialize(com_obj: *mut c_void, context: *mut c_void) -> i32;
    #[link_name = "seh_call_terminate"]
    fn seh_call_terminate(com_obj: *mut c_void) -> i32;
    #[link_name = "seh_call_get_bus_count"]
    fn seh_call_get_bus_count(com_obj: *mut c_void, media_type: i32, dir: i32) -> i32;
    #[link_name = "seh_call_activate_bus"]
    fn seh_call_activate_bus(
        com_obj: *mut c_void,
        media_type: i32,
        dir: i32,
        idx: i32,
        state: u8,
    ) -> i32;
    #[link_name = "seh_call_set_active"]
    fn seh_call_set_active(com_obj: *mut c_void, state: u8) -> i32;
    #[link_name = "seh_call_set_bus_arrangements"]
    fn seh_call_set_bus_arrangements(
        com_obj: *mut c_void,
        inputs: *mut c_void,
        num_ins: i32,
        outputs: *mut c_void,
        num_outs: i32,
    ) -> i32;
    #[link_name = "seh_call_setup_processing"]
    fn seh_call_setup_processing(com_obj: *mut c_void, setup: *mut c_void) -> i32;
    #[link_name = "seh_call_set_processing"]
    fn seh_call_set_processing(com_obj: *mut c_void, state: u8) -> i32;
    #[link_name = "seh_call_process_robust"]
    fn seh_call_process_robust(
        com_obj: *mut c_void,
        samples: i32,
        n_ins: i32,
        ins: *mut c_void,
        n_outs: i32,
        outs: *mut c_void,
        ctx: *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_set_component_handler"]
    fn seh_call_set_component_handler(com_obj: *mut c_void, handler: *mut c_void) -> i32;
    #[link_name = "seh_call_get_parameter_count"]
    fn seh_call_get_parameter_count(com_obj: *mut c_void) -> i32;
    #[link_name = "seh_call_get_parameter_info"]
    fn seh_call_get_parameter_info(com_obj: *mut c_void, idx: i32, info: *mut c_void) -> i32;
    #[link_name = "seh_call_get_param_normalized"]
    fn seh_call_get_param_normalized(com_obj: *mut c_void, id: u32) -> f64;
    #[link_name = "seh_call_set_param_normalized"]
    fn seh_call_set_param_normalized(com_obj: *mut c_void, id: u32, val: f64) -> i32;
    #[link_name = "seh_call_set_io_mode"]
    fn seh_call_set_io_mode(com_obj: *mut c_void, mode: i32) -> i32;
}

const SEH_CAUGHT: i32 = -2;

impl LoadedPlugin {
    pub fn load(info: &PluginInfo) -> anyhow::Result<Self> {
        let dll_path = Self::find_dll(&info.path)?;
        let library = unsafe { libloading::Library::new(&dll_path)? };
        let get_factory: libloading::Symbol<unsafe extern "system" fn() -> *mut c_void> =
            unsafe { library.get(b"GetPluginFactory")? };
        let factory_ptr = unsafe { get_factory() };
        if factory_ptr.is_null() {
            return Err(anyhow::anyhow!("Factory null"));
        }

        let class_count = unsafe { seh_call_count_classes(factory_ptr) };
        if class_count <= 0 {
            return Err(anyhow::anyhow!("No classes"));
        }

        let mut target_cid = None;
        for i in 0..class_count {
            let mut class_info: PClassInfo = unsafe { std::mem::zeroed() };
            if unsafe {
                seh_call_get_class_info(factory_ptr, i, &mut class_info as *mut _ as *mut _)
            } == kResultOk
            {
                let cat = String::from_utf8_lossy(unsafe {
                    std::mem::transmute::<&[i8], &[u8]>(&class_info.category)
                });
                if cat.contains("Audio") || cat.contains("Module") {
                    target_cid = Some(class_info.cid);
                    break;
                }
            }
        }
        let cid = target_cid.ok_or_else(|| anyhow::anyhow!("No CID"))?;

        let mut obj_ptr: *mut c_void = std::ptr::null_mut();
        unsafe {
            seh_call_create_instance(
                factory_ptr,
                cid.as_ptr() as *const _,
                <IComponent as Interface>::IID.as_ptr() as *const _,
                &mut obj_ptr,
            );
        }
        if obj_ptr.is_null() {
            return Err(anyhow::anyhow!("Instance null"));
        }

        let component = unsafe { ComPtr::from_raw(obj_ptr as *mut IComponent).unwrap() };
        let host_application = Box::into_raw(Box::new(HostApplication {
            vtbl: &HOST_APPLICATION_VTBL,
            ref_count: AtomicUsize::new(1),
        })) as *mut c_void;
        unsafe {
            let res = seh_call_initialize(obj_ptr, host_application);
            if res != kResultOk {
                log::warn!(
                    "IComponent::initialize() returned {} for '{}'",
                    res,
                    info.name
                );
            }
        }

        let num_inputs_raw =
            unsafe { seh_call_get_bus_count(obj_ptr, MEDIA_TYPE_AUDIO, BUS_DIR_INPUT) };
        if num_inputs_raw == SEH_CAUGHT {
            log::error!("get_bus_count(input) SEH crash for '{}'", info.name);
        }
        let num_inputs = num_inputs_raw.max(1);
        let num_outputs_raw =
            unsafe { seh_call_get_bus_count(obj_ptr, MEDIA_TYPE_AUDIO, BUS_DIR_OUTPUT) };
        if num_outputs_raw == SEH_CAUGHT {
            log::error!("get_bus_count(output) SEH crash for '{}'", info.name);
        }
        let num_outputs = num_outputs_raw.max(1);

        let mut proc_ptr: *mut c_void = std::ptr::null_mut();
        unsafe {
            seh_call_query_interface(
                obj_ptr,
                <IAudioProcessor as Interface>::IID.as_ptr() as *const _,
                &mut proc_ptr,
            );
        }
        if proc_ptr.is_null() {
            return Err(anyhow::anyhow!(
                "Failed to query IAudioProcessor interface for '{}'",
                info.name
            ));
        }
        let audio_processor =
            unsafe { ComPtr::from_raw(proc_ptr as *mut IAudioProcessor).unwrap() };

        let mut edit_ptr: *mut c_void = std::ptr::null_mut();
        unsafe {
            seh_call_query_interface(
                obj_ptr,
                <IEditController as Interface>::IID.as_ptr() as *const _,
                &mut edit_ptr,
            );
        }
        if edit_ptr.is_null() {
            log::info!(
                "IEditController not available for '{}' (single-component plugin)",
                info.name
            );
        }
        let edit_controller = if !edit_ptr.is_null() {
            unsafe { ComPtr::from_raw(edit_ptr as *mut IEditController) }
        } else {
            None
        };

        let component_handler = if let Some(ref ec) = edit_controller {
            let handler = Box::into_raw(Box::new(HostComponentHandler {
                vtbl: &HOST_COMPONENT_HANDLER_VTBL,
                ref_count: AtomicUsize::new(1),
            })) as *mut c_void;
            unsafe {
                let res = seh_call_set_component_handler(ec.as_ptr() as *mut _, handler);
                if res != kResultOk {
                    log::warn!("setComponentHandler returned {} for '{}'", res, info.name);
                }
            }
            handler
        } else {
            std::ptr::null_mut()
        };

        Ok(Self {
            component,
            edit_controller,
            audio_processor,
            info: info.clone(),
            num_inputs,
            num_outputs,
            host_application,
            component_handler,
            sample_rate: 44100.0,
            block_size: 256,
            _library: library,
        })
    }

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
        ctx.state = (1 << 3) | (1 << 10); // kTempoValid | kSampleRateValid

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

    pub fn has_editor(&self) -> bool {
        self.edit_controller.is_some()
    }
    pub fn edit_controller(&self) -> Option<&ComPtr<IEditController>> {
        self.edit_controller.as_ref()
    }
    pub fn parameter_info(&self) -> Vec<ParamInfo> {
        let mut params = Vec::new();
        if let Some(ref ec) = self.edit_controller {
            let count = unsafe { seh_call_get_parameter_count(ec.as_ptr() as *mut _) };
            if count < 0 {
                log::error!(
                    "getParameterCount returned {} for '{}'",
                    count,
                    self.info.name
                );
                return params;
            }
            for i in 0..count {
                let mut info: Vst::ParameterInfo = unsafe { std::mem::zeroed() };
                let res = unsafe {
                    seh_call_get_parameter_info(
                        ec.as_ptr() as *mut _,
                        i,
                        &mut info as *mut _ as *mut _,
                    )
                };
                if res == kResultOk {
                    let len = info.title.iter().position(|&c| c == 0).unwrap_or(128);
                    params.push(ParamInfo {
                        id: info.id,
                        name: String::from_utf16_lossy(&info.title[..len]),
                    });
                } else {
                    log::warn!(
                        "getParameterInfo({}) returned {} for '{}'",
                        i,
                        res,
                        self.info.name
                    );
                }
            }
        }
        params
    }
    pub fn get_parameter(&self, index: usize) -> f32 {
        if let Some(ref ec) = self.edit_controller {
            let info = self.parameter_info();
            if let Some(p) = info.get(index) {
                return unsafe { seh_call_get_param_normalized(ec.as_ptr() as *mut _, p.id) }
                    as f32;
            }
        }
        0.0
    }
    pub fn set_parameter(&mut self, index: usize, value: f32) {
        if let Some(ref ec) = self.edit_controller {
            let info = self.parameter_info();
            if let Some(p) = info.get(index) {
                let res = unsafe {
                    seh_call_set_param_normalized(ec.as_ptr() as *mut _, p.id, value as f64)
                };
                if res != kResultOk {
                    log::warn!(
                        "setParamNormalized(id={}, val={:.3}) returned {} for '{}'",
                        p.id,
                        value,
                        res,
                        self.info.name
                    );
                }
            }
        }
    }
    fn find_dll(path: &std::path::Path) -> anyhow::Result<PathBuf> {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        let contents = path.join("Contents").join("x86_64-win");
        if contents.is_dir() {
            for entry in std::fs::read_dir(contents)? {
                let p = entry?.path();
                if p.extension().map_or(false, |e| e == "vst3" || e == "dll") {
                    return Ok(p);
                }
            }
        }
        Err(anyhow::anyhow!("No DLL"))
    }
}

impl Drop for LoadedPlugin {
    fn drop(&mut self) {
        unsafe {
            let p = self.audio_processor.as_ptr() as *mut c_void;
            let c = self.component.as_ptr() as *mut c_void;
            let _ = seh_call_set_processing(p, 0);
            let _ = seh_call_set_active(c, 0);
            let _ = seh_call_terminate(c);
        }
        if !self.component_handler.is_null() {
            unsafe {
                handler_release(self.component_handler);
            }
        }
        if !self.host_application.is_null() {
            unsafe {
                host_app_release(self.host_application);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_load_and_process_vst() {
        let path = std::env::var("TEST_VST_PATH").ok().map(PathBuf::from);
        if let Some(p) = path {
            let mut plugin = LoadedPlugin::load(&PluginInfo {
                path: p,
                name: "Test".into(),
                vendor: "".into(),
                category: "".into(),
            })
            .unwrap();
            plugin.setup_processing(48000.0, 256).unwrap();
            let mut l = vec![0.0f32; 256];
            let mut r = vec![0.0f32; 256];
            let mut buffer = vec![l.as_mut_slice(), r.as_mut_slice()];
            for _ in 0..10 {
                plugin.process_in_place(&mut buffer, 256);
            }
        }
    }
}
