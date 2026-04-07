use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};

use vst3::ComPtr;
use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait};
use vst3::Steinberg::{kResultOk, tresult, uint32, TUID};
use vst3::Steinberg::{IPlugFrame, IPlugView, IPlugViewTrait, ViewRect};

#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::UpdateWindow,
    UI::WindowsAndMessaging::{
        AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, LoadCursorW,
        RegisterClassW, SetWindowPos, ShowWindow, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW,
        SWP_NOMOVE, SWP_NOZORDER, SW_SHOW, WNDCLASSW, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
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
    _this: *mut c_void,
    _iid: *const TUID,
    obj: *mut *mut c_void,
) -> tresult {
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
            let adj_w = wr.right - wr.left;
            let adj_h = wr.bottom - wr.top;
            unsafe {
                SetWindowPos(hwnd, 0, 0, 0, adj_w, adj_h, SWP_NOMOVE | SWP_NOZORDER);
            }
        }
    }
    let _ = (this, new_size);
    kResultOk
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

    #[allow(dead_code)]
    pub fn open_embedded(
        &mut self,
        edit_controller: &ComPtr<IEditController>,
        parent_hwnd: *mut c_void,
        plugin_name: &str,
    ) -> anyhow::Result<()> {
        if parent_hwnd.is_null() {
            return Err(anyhow::anyhow!("No parent HWND for embedded mode"));
        }
        self.open_internal(
            edit_controller,
            Some(parent_hwnd),
            EditorMode::Embedded,
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

        let view_ptr = unsafe { edit_controller.createView(b"editor\0".as_ptr() as *const i8) };
        if view_ptr.is_null() {
            return Err(anyhow::anyhow!("Plugin does not provide an editor view"));
        }

        let plug_view: ComPtr<IPlugView> = unsafe {
            ComPtr::from_raw(view_ptr as *mut IPlugView)
                .ok_or_else(|| anyhow::anyhow!("Failed to wrap IPlugView"))?
        };

        #[cfg(target_os = "windows")]
        {
            let supported = unsafe { plug_view.isPlatformTypeSupported(K_PLATFORM_TYPE_HWND) };
            if supported != kResultOk {
                return Err(anyhow::anyhow!("Plugin does not support HWND platform"));
            }
        }

        let (w, h) = Self::query_view_size(&plug_view).unwrap_or((600, 400));

        let frame_ptr = create_host_plug_frame();
        unsafe {
            plug_view.setFrame(frame_ptr);
        }
        self.plug_frame = frame_ptr;

        match mode {
            EditorMode::SeparateWindow => {
                #[cfg(target_os = "windows")]
                {
                    let parent = parent_hwnd.map(|p| p as HWND).unwrap_or(0);
                    let hwnd = self.create_editor_window(plugin_name, w, h, parent)?;
                    set_frame_hwnd(frame_ptr, hwnd);
                    unsafe {
                        let result = plug_view.attached(hwnd as *mut c_void, K_PLATFORM_TYPE_HWND);
                        if result != kResultOk {
                            DestroyWindow(hwnd);
                            return Err(anyhow::anyhow!("IPlugView::attached() failed"));
                        }
                    }
                    self.window_hwnd = Some(hwnd);
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = (w, h, plugin_name, parent_hwnd);
                    return Err(anyhow::anyhow!(
                        "Separate window not supported on this platform"
                    ));
                }
            }
            EditorMode::Embedded => {
                let parent = parent_hwnd.unwrap_or(std::ptr::null_mut());
                #[cfg(target_os = "windows")]
                {
                    unsafe {
                        let result = plug_view.attached(parent, K_PLATFORM_TYPE_HWND);
                        if result != kResultOk {
                            return Err(anyhow::anyhow!("IPlugView::attached() failed"));
                        }
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = parent;
                }
            }
        }

        self.plug_view = Some(plug_view);
        self.is_open = true;
        Ok(())
    }

    pub fn close(&mut self) {
        if !self.is_open {
            return;
        }

        if let Some(plug_view) = self.plug_view.take() {
            unsafe {
                let _ = plug_view.removed();
            }
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(hwnd) = self.window_hwnd.take() {
                unsafe {
                    DestroyWindow(hwnd);
                }
            }
        }

        if !self.plug_frame.is_null() {
            unsafe {
                let frame = self.plug_frame as *const HostPlugFrame;
                let vtbl = (*frame).vtbl;
                ((*vtbl).release)(self.plug_frame as *mut c_void);
            }
            self.plug_frame = std::ptr::null_mut();
        }

        self.is_open = false;
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    #[allow(dead_code)]
    pub fn mode(&self) -> EditorMode {
        self.mode
    }

    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    pub fn window_hwnd(&self) -> Option<HWND> {
        self.window_hwnd
    }

    #[allow(dead_code)]
    pub fn get_size(&self) -> Option<(i32, i32)> {
        self.plug_view
            .as_ref()
            .and_then(|v| Self::query_view_size(v))
    }

    fn query_view_size(view: &ComPtr<IPlugView>) -> Option<(i32, i32)> {
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            let result = view.getSize(&mut rect);
            if result == kResultOk {
                return Some((rect.right - rect.left, rect.bottom - rect.top));
            }
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
        let class_name = get_editor_class_name();

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
                class_name.as_ptr(),
                title_wide.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE | WS_CLIPCHILDREN,
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

        unsafe {
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);
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
        .map_err(|e| anyhow::anyhow!("Failed to get window handle: {:?}", e))?;
    match handle.as_raw() {
        raw_window_handle::RawWindowHandle::Win32(win32_handle) => {
            Ok(win32_handle.hwnd.get() as *mut c_void)
        }
        _ => Err(anyhow::anyhow!("Not a Win32 window")),
    }
}
