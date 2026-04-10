use std::ffi::c_void;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};

use vst3::Steinberg::Vst::{IAttributeList, IHostApplication, IMessage};
use vst3::Steinberg::{FUnknown, kInvalidArgument, kNoInterface, kResultOk, TUID, tresult, uint32};
use vst3::{ComWrapper, Interface};

use super::attributes::{HostAttributeList, HostMessage};
use super::seh_ffi::format_tuid;
use super::HOST_NAME;

#[repr(C)]
pub(super) struct HostComponentHandlerVtbl {
    pub query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    pub add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    pub release: unsafe extern "system" fn(*mut c_void) -> uint32,
    pub begin_edit: unsafe extern "system" fn(*mut c_void, u32) -> tresult,
    pub perform_edit: unsafe extern "system" fn(*mut c_void, u32, f64) -> tresult,
    pub end_edit: unsafe extern "system" fn(*mut c_void, u32) -> tresult,
    pub restart_component: unsafe extern "system" fn(*mut c_void, i32) -> tresult,
}

#[repr(C)]
pub(super) struct HostComponentHandler {
    pub vtbl: *const HostComponentHandlerVtbl,
    pub ref_count: AtomicUsize,
}

pub(super) static HOST_COMPONENT_HANDLER_VTBL: HostComponentHandlerVtbl =
    HostComponentHandlerVtbl {
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
    use vst3::Steinberg::Vst::IComponentHandler;
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

pub(super) unsafe extern "system" fn handler_add_ref(this: *mut c_void) -> uint32 {
    let obj = this as *const HostComponentHandler;
    unsafe { (*obj).ref_count.fetch_add(1, Ordering::Relaxed) as u32 + 1 }
}

pub(super) unsafe extern "system" fn handler_release(this: *mut c_void) -> uint32 {
    let obj = this as *const HostComponentHandler;
    let count = unsafe { (*obj).ref_count.fetch_sub(1, Ordering::Relaxed) };
    if count == 1 {
        let _ = unsafe { Box::from_raw(this as *mut HostComponentHandler) };
    }
    count.saturating_sub(1) as u32
}

#[repr(C)]
pub(super) struct HostApplicationVtbl {
    pub query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    pub add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    pub release: unsafe extern "system" fn(*mut c_void) -> uint32,
    pub get_name: unsafe extern "system" fn(*mut c_void, *mut u16) -> tresult,
    pub create_instance: unsafe extern "system" fn(
        *mut c_void,
        *const TUID,
        *const TUID,
        *mut *mut c_void,
    ) -> tresult,
}

#[repr(C)]
pub(super) struct HostApplication {
    pub vtbl: *const HostApplicationVtbl,
    pub ref_count: AtomicUsize,
}

pub(super) static HOST_APPLICATION_VTBL: HostApplicationVtbl = HostApplicationVtbl {
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

pub(super) unsafe extern "system" fn host_app_add_ref(this: *mut c_void) -> uint32 {
    let obj = this as *const HostApplication;
    unsafe { (*obj).ref_count.fetch_add(1, Ordering::Relaxed) as u32 + 1 }
}

pub(super) unsafe extern "system" fn host_app_release(this: *mut c_void) -> uint32 {
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

pub(super) unsafe extern "system" fn host_app_create_instance(
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
