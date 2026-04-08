use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::slice;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use vst3::Steinberg::Vst::{
    self, AudioBusBuffers, AudioBusBuffers__type0, IAttributeList, IAttributeListTrait,
    IAudioProcessor, IComponent, IComponentHandler, IConnectionPoint, IEditController,
    IHostApplication, IMessage, IMessageTrait, ProcessSetup,
};
use vst3::Steinberg::{
    int32, int64, kInvalidArgument, kNoInterface, kNotImplemented, kResultFalse, kResultOk,
    kResultTrue, tresult, uint32, FUnknown, IBStream, IBStreamTrait, PClassInfo, TUID,
};
use vst3::{ComPtr, ComWrapper, Interface};

use super::scanner::PluginInfo;
use crate::audio::chain::ParamInfo;

const SYMBOLIC_SAMPLE_SIZE_32: i32 = 0;
const PROCESS_MODE_REALTIME: i32 = 0;
const MEDIA_TYPE_AUDIO: i32 = 0;
const BUS_DIR_INPUT: i32 = 0;
const BUS_DIR_OUTPUT: i32 = 1;
const HOST_NAME: &str = "ToneDock";

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
    unsafe {
        obj.write(ptr::null_mut());
    }
    kNoInterface
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
    unsafe {
        obj.write(ptr::null_mut());
    }
    kNoInterface
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
    if name.is_null() {
        return kInvalidArgument;
    }
    let mut wide = [0u16; 128];
    for (dst, src) in wide
        .iter_mut()
        .zip(HOST_NAME.encode_utf16().chain(std::iter::once(0)))
    {
        *dst = src;
    }
    unsafe {
        ptr::copy_nonoverlapping(wide.as_ptr(), name, wide.len());
    }
    kResultOk
}

unsafe extern "system" fn host_app_create_instance(
    _: *mut c_void,
    cid: *const TUID,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if cid.is_null() || iid.is_null() || obj.is_null() {
        return kInvalidArgument;
    }

    unsafe {
        obj.write(ptr::null_mut());
    }

    let class_id = unsafe { &*(cid as *const [u8; 16]) };
    let interface_id = unsafe { &*(iid as *const [u8; 16]) };
    log::info!(
        "HostApplication::createInstance cid={} iid={}",
        format_tuid(class_id),
        format_tuid(interface_id)
    );

    if *interface_id == <IMessage as Interface>::IID
        || (*class_id == <IMessage as Interface>::IID
            && *interface_id == <FUnknown as Interface>::IID)
    {
        let message = ComWrapper::new(HostMessage::default());
        let message_ptr = message.to_com_ptr::<IMessage>().unwrap();
        unsafe {
            obj.write(message_ptr.into_raw() as *mut c_void);
        }
        return kResultOk;
    }

    if *interface_id == <IAttributeList as Interface>::IID
        || (*class_id == <IAttributeList as Interface>::IID
            && *interface_id == <FUnknown as Interface>::IID)
    {
        let attrs = ComWrapper::new(HostAttributeList::default());
        let attrs_ptr = attrs.to_com_ptr::<IAttributeList>().unwrap();
        unsafe {
            obj.write(attrs_ptr.into_raw() as *mut c_void);
        }
        return kResultOk;
    }

    log::warn!(
        "HostApplication::createInstance unsupported cid={} iid={}",
        format_tuid(class_id),
        format_tuid(interface_id)
    );
    kNoInterface
}

#[derive(Clone)]
enum AttributeValue {
    Int(int64),
    Float(f64),
    String(Box<[u16]>),
    Binary(Box<[u8]>),
}

#[derive(Default)]
struct HostAttributeList {
    values: Mutex<HashMap<Vec<u8>, AttributeValue>>,
}

impl vst3::Class for HostAttributeList {
    type Interfaces = (IAttributeList,);
}

impl IAttributeListTrait for HostAttributeList {
    unsafe fn setInt(&self, id: vst3::Steinberg::Vst::IAttributeList_::AttrID, value: int64) -> tresult {
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        lock_recover(&self.values).insert(key, AttributeValue::Int(value));
        kResultOk
    }

    unsafe fn getInt(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        value: *mut int64,
    ) -> tresult {
        if value.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let values = lock_recover(&self.values);
        match values.get(&key) {
            Some(AttributeValue::Int(v)) => {
                unsafe {
                    value.write(*v);
                }
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setFloat(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        value: f64,
    ) -> tresult {
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        lock_recover(&self.values).insert(key, AttributeValue::Float(value));
        kResultOk
    }

    unsafe fn getFloat(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        value: *mut f64,
    ) -> tresult {
        if value.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let values = lock_recover(&self.values);
        match values.get(&key) {
            Some(AttributeValue::Float(v)) => {
                unsafe {
                    value.write(*v);
                }
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setString(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        string: *const vst3::Steinberg::Vst::TChar,
    ) -> tresult {
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let value = unsafe { read_wide_string(string) };
        lock_recover(&self.values).insert(key, AttributeValue::String(value.into_boxed_slice()));
        kResultOk
    }

    unsafe fn getString(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        string: *mut vst3::Steinberg::Vst::TChar,
        size_in_bytes: uint32,
    ) -> tresult {
        if string.is_null() || size_in_bytes < std::mem::size_of::<u16>() as u32 {
            return kInvalidArgument;
        }
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let values = lock_recover(&self.values);
        let Some(AttributeValue::String(value)) = values.get(&key) else {
            return kResultFalse;
        };

        let capacity = (size_in_bytes as usize / std::mem::size_of::<u16>()).max(1);
        let dst = unsafe { slice::from_raw_parts_mut(string, capacity) };
        let copy_len = value.len().min(capacity.saturating_sub(1));
        dst[..copy_len].copy_from_slice(&value[..copy_len]);
        dst[copy_len] = 0;
        kResultOk
    }

    unsafe fn setBinary(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        data: *const c_void,
        size_in_bytes: uint32,
    ) -> tresult {
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        if data.is_null() && size_in_bytes != 0 {
            return kInvalidArgument;
        }
        let bytes = if size_in_bytes == 0 {
            Vec::new()
        } else {
            unsafe { slice::from_raw_parts(data as *const u8, size_in_bytes as usize) }.to_vec()
        };
        lock_recover(&self.values).insert(key, AttributeValue::Binary(bytes.into_boxed_slice()));
        kResultOk
    }

    unsafe fn getBinary(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        data: *mut *const c_void,
        size_in_bytes: *mut uint32,
    ) -> tresult {
        if data.is_null() || size_in_bytes.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let values = lock_recover(&self.values);
        let Some(AttributeValue::Binary(value)) = values.get(&key) else {
            unsafe {
                data.write(ptr::null());
                size_in_bytes.write(0);
            }
            return kResultFalse;
        };
        unsafe {
            data.write(value.as_ptr() as *const c_void);
            size_in_bytes.write(value.len() as uint32);
        }
        kResultOk
    }
}

struct HostMessage {
    message_id: Mutex<CString>,
    attributes: ComWrapper<HostAttributeList>,
}

impl Default for HostMessage {
    fn default() -> Self {
        Self {
            message_id: Mutex::new(CString::new("").unwrap()),
            attributes: ComWrapper::new(HostAttributeList::default()),
        }
    }
}

impl vst3::Class for HostMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> vst3::Steinberg::FIDString {
        lock_recover(&self.message_id).as_ptr()
    }

    unsafe fn setMessageID(&self, id: vst3::Steinberg::FIDString) {
        let value = if id.is_null() {
            CString::new("").unwrap()
        } else {
            CString::new(unsafe { CStr::from_ptr(id).to_bytes() })
                .unwrap_or_else(|_| CString::new("").unwrap())
        };
        *lock_recover(&self.message_id) = value;
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attributes
            .to_com_ptr::<IAttributeList>()
            .unwrap()
            .into_raw()
    }
}

#[derive(Default)]
struct MemoryStream {
    state: Mutex<MemoryStreamState>,
}

#[derive(Default)]
struct MemoryStreamState {
    data: Vec<u8>,
    position: usize,
}

impl MemoryStream {
    fn rewind(&self) {
        lock_recover(&self.state).position = 0;
    }
}

impl vst3::Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_read: *mut int32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let mut state = lock_recover(&self.state);
        let available = state.data.len().saturating_sub(state.position);
        let requested = num_bytes as usize;
        let count = available.min(requested);

        unsafe {
            ptr::copy_nonoverlapping(
                state.data.as_ptr().add(state.position),
                buffer as *mut u8,
                count,
            );
        }
        state.position += count;

        if !num_bytes_read.is_null() {
            unsafe {
                num_bytes_read.write(count as int32);
            }
        }
        if count == requested {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe fn write(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_written: *mut int32,
    ) -> tresult {
        if num_bytes < 0 || (buffer.is_null() && num_bytes != 0) {
            return kInvalidArgument;
        }
        let mut state = lock_recover(&self.state);
        let requested = num_bytes as usize;
        let end = state.position.saturating_add(requested);
        if end > state.data.len() {
            state.data.resize(end, 0);
        }
        if requested != 0 {
            unsafe {
                ptr::copy_nonoverlapping(
                    buffer as *const u8,
                    state.data.as_mut_ptr().add(state.position),
                    requested,
                );
            }
        }
        state.position = end;

        if !num_bytes_written.is_null() {
            unsafe {
                num_bytes_written.write(num_bytes);
            }
        }
        kResultOk
    }

    unsafe fn seek(
        &self,
        pos: int64,
        mode: int32,
        result: *mut int64,
    ) -> tresult {
        let mut state = lock_recover(&self.state);
        let base = match mode {
            x if x == vst3::Steinberg::IBStream_::IStreamSeekMode_::kIBSeekSet => 0i64,
            x if x == vst3::Steinberg::IBStream_::IStreamSeekMode_::kIBSeekCur => {
                state.position as i64
            }
            x if x == vst3::Steinberg::IBStream_::IStreamSeekMode_::kIBSeekEnd => {
                state.data.len() as i64
            }
            _ => return kInvalidArgument,
        };

        let Some(new_pos) = base.checked_add(pos) else {
            return kInvalidArgument;
        };
        if new_pos < 0 {
            return kInvalidArgument;
        }

        state.position = new_pos as usize;
        if !result.is_null() {
            unsafe {
                result.write(new_pos);
            }
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        unsafe {
            pos.write(lock_recover(&self.state).position as int64);
        }
        kResultOk
    }
}

pub struct LoadedPlugin {
    component: ComPtr<IComponent>,
    edit_controller: Option<ComPtr<IEditController>>,
    audio_processor: ComPtr<IAudioProcessor>,
    component_connection: Option<ComPtr<IConnectionPoint>>,
    controller_connection: Option<ComPtr<IConnectionPoint>>,
    separate_controller: bool,
    pub info: PluginInfo,
    pub num_inputs: i32,
    pub num_outputs: i32,
    host_application: *mut c_void,
    component_handler: *mut c_void,
    sample_rate: f64,
    block_size: i32,
    exit_dll: Option<unsafe extern "system" fn() -> bool>,
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
    #[link_name = "seh_call_get_controller_class_id"]
    fn seh_call_get_controller_class_id(com_obj: *mut c_void, class_id: *mut c_void) -> i32;
    #[link_name = "seh_call_component_get_state"]
    fn seh_call_component_get_state(com_obj: *mut c_void, state: *mut c_void) -> i32;
    #[link_name = "seh_call_set_component_state"]
    fn seh_call_set_component_state(com_obj: *mut c_void, state: *mut c_void) -> i32;
    #[link_name = "seh_call_connection_point_connect"]
    fn seh_call_connection_point_connect(com_obj: *mut c_void, other: *mut c_void) -> i32;
    #[link_name = "seh_call_connection_point_disconnect"]
    fn seh_call_connection_point_disconnect(com_obj: *mut c_void, other: *mut c_void) -> i32;
}

const SEH_CAUGHT: i32 = -2;

fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(err) => err.into_inner(),
    }
}

fn result_is_success(result: i32) -> bool {
    result == kResultOk || result == kResultTrue
}

fn format_tuid(tuid: &[u8; 16]) -> String {
    tuid.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join("")
}

unsafe fn attr_key(id: *const c_char) -> Option<Vec<u8>> {
    if id.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(id) }.to_bytes().to_vec())
}

unsafe fn read_wide_string(string: *const u16) -> Vec<u16> {
    if string.is_null() {
        return Vec::new();
    }
    let mut len = 0usize;
    while unsafe { *string.add(len) } != 0 {
        len += 1;
    }
    unsafe { slice::from_raw_parts(string, len) }.to_vec()
}

fn query_plugin_interface<T: Interface>(com_obj: *mut c_void) -> Option<ComPtr<T>> {
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

fn create_factory_instance<T: Interface>(
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
        let exit_dll = unsafe { library.get(b"ExitDll").ok().map(|sym: libloading::Symbol<unsafe extern "system" fn() -> bool>| *sym) };
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

        let audio_processor = query_plugin_interface::<IAudioProcessor>(component_ptr).ok_or_else(
            || {
                anyhow::anyhow!(
                    "Failed to query IAudioProcessor interface for '{}'",
                    info.name
                )
            },
        )?;

        let mut separate_controller = false;
        let mut edit_controller = query_plugin_interface::<IEditController>(component_ptr);

        if edit_controller.is_none() {
            let mut controller_cid: TUID = unsafe { std::mem::zeroed() };
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
            } else if controller_id_result != kNoInterface && controller_id_result != kResultFalse
            {
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
        let mut out_ptrs: Vec<*mut f32> =
            buffer.iter_mut().take(2).map(|ch| ch.as_mut_ptr()).collect();

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

#[cfg(test)]
mod tests {
    use super::*;

    fn guid_to_tuid(guid: [u8; 16]) -> TUID {
        unsafe { std::mem::transmute::<[u8; 16], TUID>(guid) }
    }

    #[test]
    fn host_application_creates_message_and_attribute_objects() {
        let host_application = Box::into_raw(Box::new(HostApplication {
            vtbl: &HOST_APPLICATION_VTBL,
            ref_count: AtomicUsize::new(1),
        })) as *mut c_void;

        let mut message_ptr = ptr::null_mut();
        let message_cid = guid_to_tuid(<IMessage as Interface>::IID);
        let message_iid = guid_to_tuid(<IMessage as Interface>::IID);
        let result = unsafe {
            host_app_create_instance(
                host_application,
                &message_cid,
                &message_iid,
                &mut message_ptr,
            )
        };
        assert_eq!(result, kResultOk);

        let message = unsafe { ComPtr::from_raw(message_ptr as *mut IMessage).unwrap() };
        unsafe {
            message.setMessageID(b"unit-test\0".as_ptr() as *const c_char);
        }
        let message_id = unsafe { CStr::from_ptr(message.getMessageID()) };
        assert_eq!(message_id.to_str().unwrap(), "unit-test");

        let attrs = unsafe { ComPtr::from_raw(message.getAttributes()).unwrap() };
        let key = b"answer\0".as_ptr() as *const c_char;
        assert_eq!(unsafe { attrs.setInt(key, 42) }, kResultOk);

        let mut value = 0i64;
        assert_eq!(unsafe { attrs.getInt(key, &mut value) }, kResultOk);
        assert_eq!(value, 42);

        unsafe {
            host_app_release(host_application);
        }
    }

    #[test]
    fn memory_stream_round_trip() {
        let stream = ComWrapper::new(MemoryStream::default());
        let stream_ptr = stream.to_com_ptr::<IBStream>().unwrap();

        let mut written = 0;
        let mut payload = [1u8, 2, 3, 4];
        assert_eq!(
            unsafe {
                stream_ptr.write(
                    payload.as_mut_ptr() as *mut c_void,
                    payload.len() as i32,
                    &mut written,
                )
            },
            kResultOk
        );
        assert_eq!(written, payload.len() as i32);

        stream.rewind();

        let mut read = 0;
        let mut out = [0u8; 4];
        assert_eq!(
            unsafe {
                stream_ptr.read(
                    out.as_mut_ptr() as *mut c_void,
                    out.len() as i32,
                    &mut read,
                )
            },
            kResultOk
        );
        assert_eq!(read, out.len() as i32);
        assert_eq!(out, payload);
    }

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
