use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use vst3::ComPtr;
use vst3::Steinberg::Vst::IEditController;
use vst3::Steinberg::{kResultOk, tresult, uint32, TUID};
use vst3::Steinberg::{FUnknown_iid, IPlugFrame_iid};
use vst3::Steinberg::{IPlugFrame, IPlugView, ViewRect};

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::UpdateWindow,
    UI::WindowsAndMessaging::{
        AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, LoadCursorW,
        RegisterClassW, SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW,
        SWP_NOMOVE, SWP_NOZORDER, SW_SHOW, WNDCLASSW, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT,
        WS_OVERLAPPEDWINDOW,
    },
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
fn get_editor_class_name() -> Vec<u16> {
    "ToneDockPluginEditor\0".encode_utf16().collect()
}

#[cfg(target_os = "windows")]
fn get_module_handle() -> windows_sys::Win32::Foundation::HMODULE {
    unsafe { windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null()) }
}

fn ensure_window_class_registered() {
    #[cfg(target_os = "windows")]
    {
        EDITOR_CLASS_REGISTERED.call_once(|| unsafe {
            let class_name = get_editor_class_name();
            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
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

#[repr(C)]
struct IPlugFrameComVtbl {
    query_interface:
        unsafe extern "system" fn(*mut c_void, *const TUID, *mut *mut c_void) -> tresult,
    add_ref: unsafe extern "system" fn(*mut c_void) -> uint32,
    release: unsafe extern "system" fn(*mut c_void) -> uint32,
    resize_view: unsafe extern "system" fn(*mut c_void, *mut IPlugView, *mut ViewRect) -> tresult,
}

#[repr(C)]
struct HostPlugFrame {
    vtbl: *const IPlugFrameComVtbl,
    ref_count: AtomicUsize,
    #[cfg(target_os = "windows")]
    hwnd: AtomicUsize,
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
        let frame = this as *mut HostPlugFrame;
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
            }
        }
    }
    kResultOk
}

#[cfg(target_os = "windows")]
fn pump_message_queue(hwnd: windows_sys::Win32::Foundation::HWND) {
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
}

const SEH_CAUGHT: i32 = -2;

fn is_valid_ptr(ptr: *mut c_void) -> bool {
    !ptr.is_null() && (ptr as isize) != (SEH_CAUGHT as isize)
}

fn plug_view_attached_seh(
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

fn plug_view_is_platform_type_supported_seh(
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

fn create_view_seh(edit_controller: &ComPtr<IEditController>, name: *const i8) -> *mut IPlugView {
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

fn plug_view_get_size_seh(plug_view: &ComPtr<IPlugView>, rect: &mut ViewRect) -> tresult {
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

fn plug_view_set_frame_seh(plug_view: &ComPtr<IPlugView>, frame: *mut IPlugFrame) -> tresult {
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

fn plug_view_removed_seh(plug_view: &ComPtr<IPlugView>) -> tresult {
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

fn create_host_plug_frame() -> *mut IPlugFrame {
    let frame = Box::new(HostPlugFrame {
        vtbl: &HOST_FRAME_VTBL,
        ref_count: AtomicUsize::new(1),
        #[cfg(target_os = "windows")]
        hwnd: AtomicUsize::new(0),
    });
    Box::into_raw(frame) as *mut IPlugFrame
}

fn release_host_plug_frame(frame_ptr: *mut IPlugFrame) {
    if frame_ptr.is_null() {
        return;
    }
    unsafe {
        let frame = frame_ptr as *const HostPlugFrame;
        ((*(*frame).vtbl).release)(frame_ptr as *mut c_void);
    }
}

#[cfg(target_os = "windows")]
fn set_frame_hwnd(frame_ptr: *mut IPlugFrame, hwnd: HWND) {
    if frame_ptr.is_null() {
        return;
    }
    unsafe {
        let frame = frame_ptr as *const HostPlugFrame;
        (*frame).hwnd.store(hwnd as usize, Ordering::Release);
    }
}

#[cfg(target_os = "windows")]
const K_PLATFORM_TYPE_HWND: *const i8 = b"HWND\0".as_ptr() as *const i8;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum EditorMode {
    SeparateWindow,
    #[allow(dead_code)]
    Embedded,
}

pub struct PluginEditor {
    plug_view: Option<ComPtr<IPlugView>>,
    plug_frame: *mut IPlugFrame,
    #[cfg(target_os = "windows")]
    window_hwnd: Option<HWND>,
    is_open: bool,
    mode: EditorMode,
}

impl PluginEditor {
    pub fn new() -> Self {
        ensure_window_class_registered();
        Self {
            plug_view: None,
            plug_frame: std::ptr::null_mut(),
            #[cfg(target_os = "windows")]
            window_hwnd: None,
            is_open: false,
            mode: EditorMode::SeparateWindow,
        }
    }

    pub fn open_separate_window(
        &mut self,
        edit_controller: &ComPtr<IEditController>,
        plugin_name: &str,
        parent_hwnd: Option<*mut c_void>,
    ) -> anyhow::Result<()> {
        self.open_internal(
            edit_controller,
            parent_hwnd,
            EditorMode::SeparateWindow,
            plugin_name,
        )
    }

    fn open_internal(
        &mut self,
        edit_controller: &ComPtr<IEditController>,
        parent_hwnd: Option<*mut c_void>,
        mode: EditorMode,
        plugin_name: &str,
    ) -> anyhow::Result<()> {
        if self.is_open {
            self.close();
        }
        self.mode = mode;

        log::info!("open_internal: calling createView for '{}'", plugin_name);
        let view_ptr = create_view_seh(edit_controller, b"editor\0".as_ptr() as *const i8);
        if view_ptr.is_null() {
            log::error!("open_internal: createView returned null or crashed");
            return Err(anyhow::anyhow!(
                "Plugin editor view failed to create or crashed"
            ));
        }
        log::info!("open_internal: createView returned {:p}", view_ptr);

        let plug_view = unsafe { ComPtr::from_raw(view_ptr as *mut IPlugView).unwrap() };

        #[cfg(target_os = "windows")]
        {
            let supported =
                plug_view_is_platform_type_supported_seh(&plug_view, K_PLATFORM_TYPE_HWND);
            log::info!(
                "open_internal: isPlatformTypeSupported returned {}",
                supported
            );
            if supported != kResultOk {
                return Err(anyhow::anyhow!("Plugin does not support HWND"));
            }
        }

        let (w, h) = Self::query_view_size(&plug_view).unwrap_or((600, 400));
        log::info!("open_internal: initial view size {}x{}", w, h);

        let frame_ptr = create_host_plug_frame();
        log::info!("open_internal: calling setFrame {:p}", frame_ptr);
        let sf_res = plug_view_set_frame_seh(&plug_view, frame_ptr);
        if sf_res != kResultOk {
            log::warn!("open_internal: setFrame returned {}", sf_res);
        }

        let attached_result = match mode {
            EditorMode::SeparateWindow => {
                #[cfg(target_os = "windows")]
                {
                    let hwnd = self.create_editor_window(plugin_name, w, h, 0)?;
                    set_frame_hwnd(frame_ptr, hwnd);
                    unsafe {
                        ShowWindow(hwnd, SW_SHOW);
                        UpdateWindow(hwnd);
                    }
                    pump_message_queue(hwnd);

                    log::info!(
                        "open_internal: calling attached hwnd={:p}",
                        hwnd as *mut c_void
                    );
                    let result = plug_view_attached_seh(
                        &plug_view,
                        hwnd as *mut c_void,
                        K_PLATFORM_TYPE_HWND,
                    );
                    if result == SEH_CAUGHT {
                        log::error!(
                            "open_internal: attached() SEH crash in plugin '{}' (hwnd={:p})",
                            plugin_name,
                            hwnd as *mut c_void
                        );
                    } else if result != kResultOk {
                        log::error!(
                            "open_internal: attached() returned {} for '{}'",
                            result,
                            plugin_name
                        );
                    } else {
                        log::info!("open_internal: attached returned {}", result);
                    }
                    if result != kResultOk {
                        let _ = plug_view_removed_seh(&plug_view);
                        unsafe {
                            DestroyWindow(hwnd);
                        }
                        release_host_plug_frame(frame_ptr);
                        let msg = if result == SEH_CAUGHT {
                            format!(
                                "IPlugView::attached() crashed (SEH) in plugin '{}'",
                                plugin_name
                            )
                        } else {
                            format!(
                                "IPlugView::attached() failed (result={}) in plugin '{}'",
                                result, plugin_name
                            )
                        };
                        return Err(anyhow::anyhow!("{}", msg));
                    }
                    pump_message_queue(hwnd);
                    self.window_hwnd = Some(hwnd);
                    Ok(())
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = frame_ptr;
                    Err(anyhow::anyhow!("Not supported"))
                }
            }
            EditorMode::Embedded => {
                let parent = parent_hwnd.unwrap_or(std::ptr::null_mut());
                #[cfg(target_os = "windows")]
                {
                    let result = plug_view_attached_seh(&plug_view, parent, K_PLATFORM_TYPE_HWND);
                    if result != kResultOk {
                        let _ = plug_view_removed_seh(&plug_view);
                        release_host_plug_frame(frame_ptr);
                        return Err(anyhow::anyhow!(
                            "attached failed (result={}) in plugin '{}'",
                            result,
                            plugin_name
                        ));
                    }
                    Ok(())
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = (parent, frame_ptr);
                    Err(anyhow::anyhow!("Not supported"))
                }
            }
        };

        if let Err(e) = attached_result {
            return Err(e);
        }

        self.plug_frame = frame_ptr;
        self.plug_view = Some(plug_view);
        self.is_open = true;
        log::info!(
            "open_internal: editor opened successfully for '{}'",
            plugin_name
        );
        Ok(())
    }

    pub fn close(&mut self) {
        if !self.is_open {
            return;
        }

        let window_valid = self.is_window_valid();

        if let Some(plug_view) = self.plug_view.take() {
            if window_valid {
                let _ = plug_view_removed_seh(&plug_view);
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(hwnd) = self.window_hwnd.take() {
                if window_valid {
                    unsafe {
                        DestroyWindow(hwnd);
                    }
                }
            }
        }
        release_host_plug_frame(self.plug_frame);
        self.plug_frame = std::ptr::null_mut();
        self.is_open = false;
    }

    pub fn is_open(&self) -> bool {
        if !self.is_open {
            return false;
        }
        if !self.is_window_valid() {
            return false;
        }
        true
    }

    #[cfg(target_os = "windows")]
    fn is_window_valid(&self) -> bool {
        self.window_hwnd
            .map_or(false, |hwnd| unsafe { IsWindow(hwnd) } != 0)
    }

    #[cfg(not(target_os = "windows"))]
    fn is_window_valid(&self) -> bool {
        true
    }

    fn query_view_size(view: &ComPtr<IPlugView>) -> Option<(i32, i32)> {
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if plug_view_get_size_seh(view, &mut rect) == kResultOk {
            return Some((rect.right - rect.left, rect.bottom - rect.top));
        }
        None
    }

    #[cfg(target_os = "windows")]
    fn create_editor_window(
        &self,
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
}

impl Drop for PluginEditor {
    fn drop(&mut self) {
        self.close();
    }
}

pub fn extract_hwnd_from_frame(frame: &eframe::Frame) -> anyhow::Result<*mut c_void> {
    use raw_window_handle::HasWindowHandle;
    let handle = frame
        .window_handle()
        .map_err(|e| anyhow::anyhow!("Failed handle: {:?}", e))?;
    match handle.as_raw() {
        raw_window_handle::RawWindowHandle::Win32(win32_handle) => {
            Ok(win32_handle.hwnd.get() as *mut c_void)
        }
        _ => Err(anyhow::anyhow!("Not Win32")),
    }
}
