use std::ffi::c_void;

use vst3::ComPtr;
use vst3::Steinberg::tresult;
use vst3::Steinberg::Vst::IEditController;
use vst3::Steinberg::{IPlugFrame, IPlugView, ViewRect};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CreateWindowExW, DefWindowProcW, LoadCursorW, RegisterClassW, CS_DBLCLKS,
    CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, WNDCLASSW, WS_CHILD,
    WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_CONTROLPARENT, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

static EDITOR_CLASS_REGISTERED: std::sync::Once = std::sync::Once::new();

#[cfg(target_os = "windows")]
unsafe extern "system" fn editor_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    const WM_CLOSE: u32 = 0x0010;
    if msg == WM_CLOSE {
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(target_os = "windows")]
pub(crate) fn get_editor_class_name() -> Vec<u16> {
    "ToneDockPluginEditor\0".encode_utf16().collect()
}

#[cfg(target_os = "windows")]
pub(crate) fn get_module_handle() -> windows_sys::Win32::Foundation::HMODULE {
    unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) }
}

pub(crate) fn ensure_window_class_registered() {
    #[cfg(target_os = "windows")]
    {
        EDITOR_CLASS_REGISTERED.call_once(|| unsafe {
            let class_name = get_editor_class_name();
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS | CS_OWNDC,
                lpfnWndProc: Some(editor_wnd_proc),
                hInstance: get_module_handle(),
                lpszClassName: class_name.as_ptr(),
                hCursor: LoadCursorW(0, IDC_ARROW),
                hbrBackground: 0,
                cbClsExtra: 0,
                cbWndExtra: 0,
                lpszMenuName: std::ptr::null(),
                hIcon: 0,
            };
            RegisterClassW(&wc);
        });
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn pump_message_queue(hwnd: HWND) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "C" {
    #[link_name = "seh_call_plug_view_attached"]
    fn seh_call_plug_view_attached(
        com_obj: *mut c_void,
        parent: *mut c_void,
        type_: *const i8,
    ) -> i32;
    #[link_name = "seh_call_is_platform_type_supported"]
    fn seh_call_is_platform_type_supported(com_obj: *mut c_void, type_: *const i8) -> i32;
    #[link_name = "seh_call_create_view"]
    fn seh_call_create_view(com_obj: *mut c_void, name: *const i8) -> *mut c_void;
    #[link_name = "seh_call_plug_view_get_size"]
    fn seh_call_plug_view_get_size(com_obj: *mut c_void, rect: *mut c_void) -> i32;
    #[link_name = "seh_call_plug_view_set_frame"]
    fn seh_call_plug_view_set_frame(com_obj: *mut c_void, frame: *mut c_void) -> i32;
    #[link_name = "seh_call_plug_view_removed"]
    fn seh_call_plug_view_removed(com_obj: *mut c_void) -> i32;
    #[link_name = "seh_get_last_exception_code"]
    fn seh_get_last_exception_code() -> u32;
    #[link_name = "seh_get_last_exception_address"]
    fn seh_get_last_exception_address() -> *mut c_void;
    #[link_name = "seh_get_last_exception_rdi"]
    fn seh_get_last_exception_rdi() -> u64;
    #[link_name = "seh_get_last_exception_rax"]
    fn seh_get_last_exception_rax() -> u64;
    #[link_name = "seh_get_last_exception_rdx"]
    fn seh_get_last_exception_rdx() -> u64;
}

const SEH_CAUGHT: i32 = -2;

pub(crate) fn is_valid_ptr(ptr: *mut c_void) -> bool {
    !ptr.is_null() && (ptr as isize) != (SEH_CAUGHT as isize)
}

pub(crate) fn plug_view_attached_seh(
    plug_view: &ComPtr<IPlugView>,
    parent: *mut c_void,
    platform_type: *const i8,
) -> tresult {
    #[cfg(target_os = "windows")]
    {
        let com_obj = plug_view.as_ptr() as *mut c_void;
        if !is_valid_ptr(com_obj) {
            return -1;
        }
        unsafe { seh_call_plug_view_attached(com_obj, parent, platform_type) }
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { plug_view.attached(parent, platform_type) }
    }
}

pub(crate) fn plug_view_is_platform_type_supported_seh(
    plug_view: &ComPtr<IPlugView>,
    platform_type: *const i8,
) -> tresult {
    #[cfg(target_os = "windows")]
    {
        let com_obj = plug_view.as_ptr() as *mut c_void;
        if !is_valid_ptr(com_obj) {
            return -1;
        }
        unsafe { seh_call_is_platform_type_supported(com_obj, platform_type) }
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { plug_view.isPlatformTypeSupported(platform_type) }
    }
}

pub(crate) fn create_view_seh(
    edit_controller: &ComPtr<IEditController>,
    name: *const i8,
) -> *mut IPlugView {
    #[cfg(target_os = "windows")]
    {
        let com_obj = edit_controller.as_ptr() as *mut c_void;
        if !is_valid_ptr(com_obj) {
            return std::ptr::null_mut();
        }
        let result = unsafe { seh_call_create_view(com_obj, name) };
        if !is_valid_ptr(result) {
            return std::ptr::null_mut();
        }
        result as *mut IPlugView
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { edit_controller.createView(name) }
    }
}

pub(crate) fn plug_view_get_size_seh(
    plug_view: &ComPtr<IPlugView>,
    rect: &mut ViewRect,
) -> tresult {
    #[cfg(target_os = "windows")]
    {
        let com_obj = plug_view.as_ptr() as *mut c_void;
        if !is_valid_ptr(com_obj) {
            return -1;
        }
        unsafe { seh_call_plug_view_get_size(com_obj, rect as *mut _ as *mut c_void) }
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { plug_view.getSize(rect) }
    }
}

pub(crate) fn plug_view_set_frame_seh(
    plug_view: &ComPtr<IPlugView>,
    frame: *mut IPlugFrame,
) -> tresult {
    #[cfg(target_os = "windows")]
    {
        let com_obj = plug_view.as_ptr() as *mut c_void;
        if !is_valid_ptr(com_obj) {
            return -1;
        }
        unsafe { seh_call_plug_view_set_frame(com_obj, frame as *mut c_void) }
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { plug_view.setFrame(frame) }
    }
}

pub(crate) fn plug_view_removed_seh(plug_view: &ComPtr<IPlugView>) -> tresult {
    #[cfg(target_os = "windows")]
    {
        let com_obj = plug_view.as_ptr() as *mut c_void;
        if !is_valid_ptr(com_obj) {
            return -1;
        }
        unsafe { seh_call_plug_view_removed(com_obj) }
    }
    #[cfg(not(target_os = "windows"))]
    {
        unsafe { plug_view.removed() }
    }
}

#[cfg(target_os = "windows")]
pub(crate) const K_PLATFORM_TYPE_HWND: *const i8 = b"HWND\0".as_ptr() as *const i8;

#[cfg(target_os = "windows")]
pub(crate) fn seh_get_diagnostics() -> (u32, *mut c_void, u64, u64, u64) {
    unsafe {
        (
            seh_get_last_exception_code(),
            seh_get_last_exception_address(),
            seh_get_last_exception_rdi(),
            seh_get_last_exception_rax(),
            seh_get_last_exception_rdx(),
        )
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn seh_dump_rdi(rdi: u64) -> [u32; 16] {
    let rdi_ptr = rdi as *const u32;
    let mut rdi_dump = [0u32; 16];
    unsafe {
        for k in 0..16 {
            let src = rdi_ptr.add(k);
            rdi_dump[k] = if src as usize > 0x10000 { *src } else { 0 };
        }
    }
    rdi_dump
}

#[cfg(target_os = "windows")]
pub(crate) const SEH_CAUGHT_SENTINEL: i32 = SEH_CAUGHT;

#[cfg(target_os = "windows")]
pub(crate) fn create_editor_window(
    title: &str,
    width: i32,
    height: i32,
    parent_hwnd: HWND,
) -> anyhow::Result<HWND> {
    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let mut wr = windows_sys::Win32::Foundation::RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    unsafe {
        AdjustWindowRect(&mut wr, WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN, 0);
    }
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_CONTROLPARENT,
            get_editor_class_name().as_ptr(),
            title_wide.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            wr.right - wr.left,
            wr.bottom - wr.top,
            parent_hwnd,
            0,
            get_module_handle(),
            std::ptr::null_mut(),
        )
    };
    if hwnd == 0 {
        return Err(anyhow::anyhow!("CreateWindowExW failed"));
    }
    Ok(hwnd)
}

#[cfg(target_os = "windows")]
pub(crate) fn create_embedded_window(
    parent_hwnd: HWND,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> anyhow::Result<HWND> {
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            get_editor_class_name().as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            x,
            y,
            width,
            height,
            parent_hwnd,
            0,
            get_module_handle(),
            std::ptr::null_mut(),
        )
    };
    if hwnd == 0 {
        return Err(anyhow::anyhow!(
            "CreateWindowExW failed for embedded editor"
        ));
    }
    Ok(hwnd)
}
