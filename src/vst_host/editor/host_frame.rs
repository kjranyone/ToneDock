use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use vst3::Steinberg::{kResultOk, tresult, uint32, TUID};
use vst3::Steinberg::{FUnknown_iid, IPlugFrame_iid};
use vst3::Steinberg::{IPlugFrame, IPlugView, ViewRect};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, MoveWindow, SetWindowPos, SWP_NOMOVE, SWP_NOZORDER, WS_CLIPCHILDREN,
    WS_OVERLAPPEDWINDOW,
};

#[repr(C)]
pub(crate) struct IPlugFrameComVtbl {
    pub query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    pub add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    pub release: unsafe extern "system" fn(*mut c_void) -> uint32,
    pub resize_view:
        unsafe extern "system" fn(*mut c_void, *mut IPlugView, *mut ViewRect) -> tresult,
}

#[repr(C)]
pub(crate) struct HostPlugFrame {
    pub vtbl: *const IPlugFrameComVtbl,
    pub ref_count: AtomicUsize,
    #[cfg(target_os = "windows")]
    pub hwnd: AtomicUsize,
}

unsafe impl Send for HostPlugFrame {}
unsafe impl Sync for HostPlugFrame {}

static HOST_FRAME_VTBL: IPlugFrameComVtbl = IPlugFrameComVtbl {
    query_interface: host_frame_qi,
    add_ref: host_frame_add_ref,
    release: host_frame_release,
    resize_view: host_frame_resize_view,
};

unsafe extern "system" fn host_frame_qi(
    this: *mut c_void,
    iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
    if iid.is_null() || obj.is_null() {
        return -1;
    }
    let iid_val = unsafe { &*iid };
    if *iid_val == FUnknown_iid || *iid_val == IPlugFrame_iid {
        unsafe {
            host_frame_add_ref(this);
            obj.write(this);
        }
        return kResultOk;
    }
    unsafe { obj.write(std::ptr::null_mut()) };
    -1
}

unsafe extern "system" fn host_frame_add_ref(this: *mut c_void) -> uint32 {
    unsafe {
        let frame = this as *const HostPlugFrame;
        (*frame).ref_count.fetch_add(1, Ordering::Relaxed) as u32 + 1
    }
}

unsafe extern "system" fn host_frame_release(this: *mut c_void) -> uint32 {
    unsafe {
        let frame = this as *const HostPlugFrame;
        let count = (*frame).ref_count.fetch_sub(1, Ordering::Relaxed);
        if count == 1 {
            let _ = Box::from_raw(this as *mut HostPlugFrame);
        }
        (count.saturating_sub(1)) as u32
    }
}

unsafe extern "system" fn host_frame_resize_view(
    this: *mut c_void,
    _view: *mut IPlugView,
    new_size: *mut ViewRect,
) -> tresult {
    #[cfg(target_os = "windows")]
    {
        let frame = this as *const HostPlugFrame;
        let hwnd = unsafe { (*frame).hwnd.load(Ordering::Acquire) } as HWND;
        if hwnd != 0 && !new_size.is_null() {
            let rect = unsafe { &*new_size };
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            let mut wr = windows_sys::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            };
            unsafe {
                AdjustWindowRect(&mut wr, WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN, 0);
            }
            unsafe {
                SetWindowPos(
                    hwnd,
                    0,
                    0,
                    0,
                    wr.right - wr.left,
                    wr.bottom - wr.top,
                    SWP_NOMOVE | SWP_NOZORDER,
                );
                let child = windows_sys::Win32::UI::WindowsAndMessaging::GetWindow(
                    hwnd,
                    windows_sys::Win32::UI::WindowsAndMessaging::GW_CHILD,
                );
                if child != 0 {
                    MoveWindow(child, 0, 0, w, h, 1);
                }
            }
        }
    }
    kResultOk
}

pub(crate) fn create_host_plug_frame() -> *mut IPlugFrame {
    let frame = Box::new(HostPlugFrame {
        vtbl: &HOST_FRAME_VTBL,
        ref_count: AtomicUsize::new(1),
        #[cfg(target_os = "windows")]
        hwnd: AtomicUsize::new(0),
    });
    Box::into_raw(frame) as *mut IPlugFrame
}

pub(crate) fn release_host_plug_frame(frame_ptr: *mut IPlugFrame) {
    if frame_ptr.is_null() {
        return;
    }
    unsafe {
        let frame = frame_ptr as *const HostPlugFrame;
        ((*(*frame).vtbl).release)(frame_ptr as *mut c_void);
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn set_frame_hwnd(frame_ptr: *mut IPlugFrame, hwnd: HWND) {
    if frame_ptr.is_null() {
        return;
    }
    unsafe {
        let frame = frame_ptr as *const HostPlugFrame;
        (*frame).hwnd.store(hwnd as usize, Ordering::Release);
    }
}
