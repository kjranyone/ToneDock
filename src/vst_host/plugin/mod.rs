mod attributes;
mod host_impl;
mod parameters;
mod processing;
mod seh_ffi;

#[cfg(test)]
mod tests;

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::AtomicUsize;

use vst3::Steinberg::Vst::{IAudioProcessor, IComponent, IConnectionPoint, IEditController};
use vst3::Steinberg::{
    kNoInterface, kNotImplemented, kResultFalse, kResultOk, IBStream, PClassInfo,
};
use vst3::{ComPtr, ComWrapper};

use super::scanner::PluginInfo;

use attributes::MemoryStream;
use host_impl::*;
use seh_ffi::*;

const SYMBOLIC_SAMPLE_SIZE_32: i32 = 0;
const PROCESS_MODE_REALTIME: i32 = 0;
const MEDIA_TYPE_AUDIO: i32 = 0;
const BUS_DIR_INPUT: i32 = 0;
const BUS_DIR_OUTPUT: i32 = 1;
const HOST_NAME: &str = "ToneDock";

pub struct LoadedPlugin {
    pub(super) component: ComPtr<IComponent>,
    pub(super) edit_controller: Option<ComPtr<IEditController>>,
    pub(super) audio_processor: ComPtr<IAudioProcessor>,
    component_connection: Option<ComPtr<IConnectionPoint>>,
    controller_connection: Option<ComPtr<IConnectionPoint>>,
    separate_controller: bool,
    pub info: PluginInfo,
    pub num_inputs: i32,
    pub num_outputs: i32,
    host_application: *mut c_void,
    component_handler: *mut c_void,
    pub(super) sample_rate: f64,
    pub(super) block_size: i32,
    exit_dll: Option<unsafe extern "system" fn() -> bool>,
    _library: libloading::Library,
}

unsafe impl Send for LoadedPlugin {}

fn sync_component_state(
    component: &ComPtr<IComponent>,
    controller: &ComPtr<IEditController>,
    plugin_name: &str,
) {
    let stream = ComWrapper::new(MemoryStream::default());
    let stream_ptr = stream.to_com_ptr::<IBStream>().unwrap();

    let get_state = unsafe {
        seh_call_component_get_state(
            component.as_ptr() as *mut c_void,
            stream_ptr.as_ptr() as *mut c_void,
        )
    };
    if !result_is_success(get_state) {
        if get_state != kNotImplemented && get_state != kResultFalse {
            log::warn!("getState returned {} for '{}'", get_state, plugin_name);
        }
        return;
    }

    stream.rewind();
    let set_state = unsafe {
        seh_call_set_component_state(
            controller.as_ptr() as *mut c_void,
            stream_ptr.as_ptr() as *mut c_void,
        )
    };
    if !result_is_success(set_state) {
        log::warn!(
            "setComponentState returned {} for '{}'",
            set_state,
            plugin_name
        );
    }
}

impl LoadedPlugin {
    pub fn load(info: &PluginInfo) -> anyhow::Result<Self> {
        let dll_path = Self::find_dll(&info.path)?;
        let library = unsafe { libloading::Library::new(&dll_path)? };
        let init_dll: Option<libloading::Symbol<unsafe extern "system" fn() -> bool>> =
            unsafe { library.get(b"InitDll").ok() };
        if let Some(init_dll) = init_dll {
            let init_result = unsafe { init_dll() };
            if !init_result {
                log::warn!("InitDll returned false for '{}'", info.name);
            }
        }
        let exit_dll = unsafe {
            library
                .get(b"ExitDll")
                .ok()
                .map(|sym: libloading::Symbol<unsafe extern "system" fn() -> bool>| *sym)
        };
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

        let component = create_factory_instance::<IComponent>(factory_ptr, &cid)
            .ok_or_else(|| anyhow::anyhow!("Instance null"))?;
        let component_ptr = component.as_ptr() as *mut c_void;

        let host_application = Box::into_raw(Box::new(HostApplication {
            vtbl: &HOST_APPLICATION_VTBL,
            ref_count: AtomicUsize::new(1),
        })) as *mut c_void;

        let init_result = unsafe { seh_call_initialize(component_ptr, host_application) };
        if !result_is_success(init_result) {
            unsafe {
                host_app_release(host_application);
            }
            return Err(anyhow::anyhow!(
                "IComponent::initialize() returned {} for '{}'",
                init_result,
                info.name
            ));
        }

        let num_inputs_raw =
            unsafe { seh_call_get_bus_count(component_ptr, MEDIA_TYPE_AUDIO, BUS_DIR_INPUT) };
        if num_inputs_raw == SEH_CAUGHT {
            log::error!("get_bus_count(input) SEH crash for '{}'", info.name);
        }
        let num_inputs = num_inputs_raw.max(1);

        let num_outputs_raw =
            unsafe { seh_call_get_bus_count(component_ptr, MEDIA_TYPE_AUDIO, BUS_DIR_OUTPUT) };
        if num_outputs_raw == SEH_CAUGHT {
            log::error!("get_bus_count(output) SEH crash for '{}'", info.name);
        }
        let num_outputs = num_outputs_raw.max(1);

        let audio_processor =
            query_plugin_interface::<IAudioProcessor>(component_ptr).ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to query IAudioProcessor interface for '{}'",
                    info.name
                )
            })?;

        let mut separate_controller = false;
        let mut edit_controller = query_plugin_interface::<IEditController>(component_ptr);

        if edit_controller.is_none() {
            let mut controller_cid: vst3::Steinberg::TUID = unsafe { std::mem::zeroed() };
            let controller_id_result = unsafe {
                seh_call_get_controller_class_id(
                    component_ptr,
                    &mut controller_cid as *mut _ as *mut c_void,
                )
            };

            if result_is_success(controller_id_result) {
                if let Some(controller) =
                    create_factory_instance::<IEditController>(factory_ptr, &controller_cid)
                {
                    let controller_init = unsafe {
                        seh_call_initialize(controller.as_ptr() as *mut c_void, host_application)
                    };
                    if result_is_success(controller_init) {
                        separate_controller = true;
                        edit_controller = Some(controller);
                    } else {
                        log::warn!(
                            "IEditController::initialize() returned {} for '{}'",
                            controller_init,
                            info.name
                        );
                        unsafe {
                            let _ = seh_call_terminate(controller.as_ptr() as *mut c_void);
                        }
                    }
                } else {
                    log::warn!("Failed to create separate controller for '{}'", info.name);
                }
            } else if controller_id_result != kNoInterface && controller_id_result != kResultFalse {
                log::warn!(
                    "getControllerClassId returned {} for '{}'",
                    controller_id_result,
                    info.name
                );
            }
        }

        let mut component_handler = ptr::null_mut();
        if let Some(ref ec) = edit_controller {
            let handler = Box::into_raw(Box::new(HostComponentHandler {
                vtbl: &HOST_COMPONENT_HANDLER_VTBL,
                ref_count: AtomicUsize::new(1),
            })) as *mut c_void;
            let set_handler =
                unsafe { seh_call_set_component_handler(ec.as_ptr() as *mut _, handler) };
            if result_is_success(set_handler) {
                component_handler = handler;
            } else {
                log::warn!(
                    "setComponentHandler returned {} for '{}'",
                    set_handler,
                    info.name
                );
                unsafe {
                    handler_release(handler);
                }
            }
        }

        let mut component_connection = None;
        let mut controller_connection = None;
        if separate_controller {
            if let Some(ref ec) = edit_controller {
                component_connection = query_plugin_interface::<IConnectionPoint>(component_ptr);
                controller_connection =
                    query_plugin_interface::<IConnectionPoint>(ec.as_ptr() as *mut c_void);

                match (&component_connection, &controller_connection) {
                    (Some(component_cp), Some(controller_cp)) => {
                        let connect_component = unsafe {
                            seh_call_connection_point_connect(
                                component_cp.as_ptr() as *mut c_void,
                                controller_cp.as_ptr() as *mut c_void,
                            )
                        };
                        if !result_is_success(connect_component) {
                            log::warn!(
                                "component IConnectionPoint::connect returned {} for '{}'",
                                connect_component,
                                info.name
                            );
                        }

                        let connect_controller = unsafe {
                            seh_call_connection_point_connect(
                                controller_cp.as_ptr() as *mut c_void,
                                component_cp.as_ptr() as *mut c_void,
                            )
                        };
                        if !result_is_success(connect_controller) {
                            log::warn!(
                                "controller IConnectionPoint::connect returned {} for '{}'",
                                connect_controller,
                                info.name
                            );
                        }
                    }
                    _ => {
                        log::warn!("IConnectionPoint not available for '{}'", info.name);
                    }
                }

                sync_component_state(&component, ec, &info.name);
            }
        }

        Ok(Self {
            component,
            edit_controller,
            audio_processor,
            component_connection,
            controller_connection,
            separate_controller,
            info: info.clone(),
            num_inputs,
            num_outputs,
            host_application,
            component_handler,
            sample_rate: 44100.0,
            block_size: 256,
            exit_dll,
            _library: library,
        })
    }

    fn find_dll(path: &std::path::Path) -> anyhow::Result<PathBuf> {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }

        let contents = path.join("Contents").join("x86_64-win");
        if contents.is_dir() {
            for entry in std::fs::read_dir(contents)? {
                let p = entry?.path();
                if p.extension().is_some_and(|e| e == "vst3" || e == "dll") {
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
            if let (Some(component_cp), Some(controller_cp)) =
                (&self.component_connection, &self.controller_connection)
            {
                let _ = seh_call_connection_point_disconnect(
                    component_cp.as_ptr() as *mut c_void,
                    controller_cp.as_ptr() as *mut c_void,
                );
                let _ = seh_call_connection_point_disconnect(
                    controller_cp.as_ptr() as *mut c_void,
                    component_cp.as_ptr() as *mut c_void,
                );
            }

            let p = self.audio_processor.as_ptr() as *mut c_void;
            let c = self.component.as_ptr() as *mut c_void;
            let _ = seh_call_set_processing(p, 0);
            let _ = seh_call_set_active(c, 0);

            if self.separate_controller {
                if let Some(ref ec) = self.edit_controller {
                    let _ = seh_call_terminate(ec.as_ptr() as *mut c_void);
                }
            }

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

        if let Some(exit_dll) = self.exit_dll {
            unsafe {
                let _ = exit_dll();
            }
        }
    }
}
