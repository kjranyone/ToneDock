use std::ffi::c_void;
use std::ptr;
use std::sync::{Mutex, MutexGuard};

use vst3::Steinberg::{kResultOk, kResultTrue, TUID};
use vst3::{ComPtr, Interface};

pub(super) const SEH_CAUGHT: i32 = -2;

unsafe extern "C" {
    #[link_name = "seh_call_query_interface"]
    pub(super) fn seh_call_query_interface(
        com_obj: *mut c_void,
        iid: *const c_void,
        obj: *mut *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_count_classes"]
    pub(super) fn seh_call_count_classes(com_obj: *mut c_void) -> i32;
    #[link_name = "seh_call_get_class_info"]
    pub(super) fn seh_call_get_class_info(com_obj: *mut c_void, idx: i32, info: *mut c_void)
        -> i32;
    #[link_name = "seh_call_create_instance"]
    pub(super) fn seh_call_create_instance(
        com_obj: *mut c_void,
        cid: *const c_void,
        iid: *const c_void,
        obj: *mut *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_initialize"]
    pub(super) fn seh_call_initialize(com_obj: *mut c_void, context: *mut c_void) -> i32;
    #[link_name = "seh_call_terminate"]
    pub(super) fn seh_call_terminate(com_obj: *mut c_void) -> i32;
    #[link_name = "seh_call_get_bus_count"]
    pub(super) fn seh_call_get_bus_count(com_obj: *mut c_void, media_type: i32, dir: i32) -> i32;
    #[link_name = "seh_call_activate_bus"]
    pub(super) fn seh_call_activate_bus(
        com_obj: *mut c_void,
        media_type: i32,
        dir: i32,
        idx: i32,
        state: u8,
    ) -> i32;
    #[link_name = "seh_call_set_active"]
    pub(super) fn seh_call_set_active(com_obj: *mut c_void, state: u8) -> i32;
    #[link_name = "seh_call_set_bus_arrangements"]
    pub(super) fn seh_call_set_bus_arrangements(
        com_obj: *mut c_void,
        inputs: *mut c_void,
        num_ins: i32,
        outputs: *mut c_void,
        num_outs: i32,
    ) -> i32;
    #[link_name = "seh_call_setup_processing"]
    pub(super) fn seh_call_setup_processing(com_obj: *mut c_void, setup: *mut c_void) -> i32;
    #[link_name = "seh_call_set_processing"]
    pub(super) fn seh_call_set_processing(com_obj: *mut c_void, state: u8) -> i32;
    #[link_name = "seh_call_process_robust"]
    pub(super) fn seh_call_process_robust(
        com_obj: *mut c_void,
        samples: i32,
        n_ins: i32,
        ins: *mut c_void,
        n_outs: i32,
        outs: *mut c_void,
        ctx: *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_set_component_handler"]
    pub(super) fn seh_call_set_component_handler(com_obj: *mut c_void, handler: *mut c_void)
        -> i32;
    #[link_name = "seh_call_get_parameter_count"]
    pub(super) fn seh_call_get_parameter_count(com_obj: *mut c_void) -> i32;
    #[link_name = "seh_call_get_parameter_info"]
    pub(super) fn seh_call_get_parameter_info(
        com_obj: *mut c_void,
        idx: i32,
        info: *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_get_param_normalized"]
    pub(super) fn seh_call_get_param_normalized(com_obj: *mut c_void, id: u32) -> f64;
    #[link_name = "seh_call_set_param_normalized"]
    pub(super) fn seh_call_set_param_normalized(com_obj: *mut c_void, id: u32, val: f64) -> i32;
    #[link_name = "seh_call_set_io_mode"]
    pub(super) fn seh_call_set_io_mode(com_obj: *mut c_void, mode: i32) -> i32;
    #[link_name = "seh_call_get_controller_class_id"]
    pub(super) fn seh_call_get_controller_class_id(
        com_obj: *mut c_void,
        class_id: *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_component_get_state"]
    pub(super) fn seh_call_component_get_state(com_obj: *mut c_void, state: *mut c_void) -> i32;
    #[link_name = "seh_call_component_set_state"]
    pub(super) fn seh_call_component_set_state(com_obj: *mut c_void, state: *mut c_void) -> i32;
    #[link_name = "seh_call_set_component_state"]
    pub(super) fn seh_call_set_component_state(com_obj: *mut c_void, state: *mut c_void) -> i32;
    #[link_name = "seh_call_connection_point_connect"]
    pub(super) fn seh_call_connection_point_connect(
        com_obj: *mut c_void,
        other: *mut c_void,
    ) -> i32;
    #[link_name = "seh_call_connection_point_disconnect"]
    pub(super) fn seh_call_connection_point_disconnect(
        com_obj: *mut c_void,
        other: *mut c_void,
    ) -> i32;
}

pub(super) fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(err) => err.into_inner(),
    }
}

pub(super) fn result_is_success(result: i32) -> bool {
    result == kResultOk || result == kResultTrue
}

pub(super) fn format_tuid(tuid: &[u8; 16]) -> String {
    tuid.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn query_plugin_interface<T: Interface>(com_obj: *mut c_void) -> Option<ComPtr<T>> {
    let mut ptr: *mut c_void = ptr::null_mut();
    let result = unsafe {
        seh_call_query_interface(
            com_obj,
            <T as Interface>::IID.as_ptr() as *const c_void,
            &mut ptr,
        )
    };
    if result == SEH_CAUGHT || ptr.is_null() {
        return None;
    }
    unsafe { ComPtr::from_raw(ptr as *mut T) }
}

pub(super) fn create_factory_instance<T: Interface>(
    factory_ptr: *mut c_void,
    cid: &TUID,
) -> Option<ComPtr<T>> {
    let mut obj_ptr: *mut c_void = ptr::null_mut();
    unsafe {
        seh_call_create_instance(
            factory_ptr,
            cid.as_ptr() as *const c_void,
            <T as Interface>::IID.as_ptr() as *const c_void,
            &mut obj_ptr,
        );
    }
    if obj_ptr.is_null() {
        return None;
    }
    unsafe { ComPtr::from_raw(obj_ptr as *mut T) }
}
