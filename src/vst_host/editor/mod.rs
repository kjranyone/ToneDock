use std::ffi::c_void;

use vst3::ComPtr;
use vst3::Steinberg::kResultOk;
use vst3::Steinberg::Vst::IEditController;
use vst3::Steinberg::{IPlugFrame, IPlugView, ViewRect};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::UpdateWindow;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, MoveWindow, ShowWindow, SW_SHOW, WS_CHILD, WS_CLIPCHILDREN,
    WS_CLIPSIBLINGS, WS_VISIBLE,
};

mod host_frame;
mod win32;

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
    owns_window: bool,
    preferred_size: (i32, i32),
}

impl PluginEditor {
    pub fn new() -> Self {
        win32::ensure_window_class_registered();
        Self {
            plug_view: None,
            plug_frame: std::ptr::null_mut(),
            #[cfg(target_os = "windows")]
            window_hwnd: None,
            is_open: false,
            mode: EditorMode::SeparateWindow,
            owns_window: false,
            preferred_size: (600, 400),
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

    pub fn open_embedded_window(
        &mut self,
        edit_controller: &ComPtr<IEditController>,
        plugin_name: &str,
        parent_hwnd: *mut c_void,
        bounds: (i32, i32, i32, i32),
    ) -> anyhow::Result<()> {
        if self.is_open {
            self.close();
        }

        #[cfg(target_os = "windows")]
        {
            let parent_hwnd = parent_hwnd as HWND;
            let hwnd =
                win32::create_embedded_window(parent_hwnd, bounds.0, bounds.1, bounds.2, bounds.3)?;
            self.window_hwnd = Some(hwnd);
            self.owns_window = true;
            let result = self.open_internal(
                edit_controller,
                Some(hwnd as *mut c_void),
                EditorMode::Embedded,
                plugin_name,
            );
            if result.is_err() {
                if let Some(h) = self.window_hwnd.take() {
                    if unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(h) } != 0 {
                        unsafe {
                            DestroyWindow(h);
                        }
                    }
                }
                self.owns_window = false;
            }
            return result;
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = (edit_controller, plugin_name, parent_hwnd, bounds);
            Err(anyhow::anyhow!(
                "Embedded editor not supported on this platform"
            ))
        }
    }

    pub fn set_embedded_bounds(&mut self, bounds: (i32, i32, i32, i32)) {
        #[cfg(target_os = "windows")]
        {
            if self.mode == EditorMode::Embedded {
                if let Some(hwnd) = self.window_hwnd {
                    unsafe {
                        MoveWindow(hwnd, bounds.0, bounds.1, bounds.2, bounds.3, 1);
                        ShowWindow(hwnd, SW_SHOW);
                        UpdateWindow(hwnd);
                    }
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        let _ = bounds;
    }

    pub fn preferred_size(&self) -> (i32, i32) {
        self.preferred_size
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
        if mode == EditorMode::SeparateWindow {
            self.owns_window = true;
        }

        #[cfg(target_os = "windows")]
        {
            let hr = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
            if hr as u32 == 0x80010106u32 {
                log::warn!(
                    "open_internal: CoInitializeEx returned RPC_E_CHANGED_MODE (thread already in MTA)"
                );
            } else if hr < 0 {
                log::warn!("open_internal: CoInitializeEx returned 0x{:08X}", hr as u32);
            } else {
                log::info!("open_internal: CoInitializeEx succeeded (STA)");
            }
        }

        log::info!("open_internal: calling createView for '{}'", plugin_name);
        let view_ptr = win32::create_view_seh(edit_controller, b"editor\0".as_ptr() as *const i8);
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
            let supported = win32::plug_view_is_platform_type_supported_seh(
                &plug_view,
                win32::K_PLATFORM_TYPE_HWND,
            );
            log::info!(
                "open_internal: isPlatformTypeSupported returned {}",
                supported
            );
            if supported != kResultOk {
                return Err(anyhow::anyhow!("Plugin does not support HWND"));
            }
        }

        let (w, h) = Self::query_view_size(&plug_view).unwrap_or((600, 400));
        self.preferred_size = (w, h);
        log::info!("open_internal: initial view size {}x{}", w, h);

        let frame_ptr = host_frame::create_host_plug_frame();
        log::info!("open_internal: calling setFrame {:p}", frame_ptr);
        let sf_res = win32::plug_view_set_frame_seh(&plug_view, frame_ptr);
        if sf_res != kResultOk {
            log::warn!("open_internal: setFrame returned {}", sf_res);
        }

        let attached_result = match mode {
            EditorMode::SeparateWindow => {
                #[cfg(target_os = "windows")]
                {
                    let owner = parent_hwnd.map(|p| p as HWND).unwrap_or(0);
                    let hwnd = win32::create_editor_window(plugin_name, w, h, owner)?;
                    host_frame::set_frame_hwnd(frame_ptr, hwnd);

                    let child_hwnd = unsafe {
                        CreateWindowExW(
                            0,
                            win32::get_editor_class_name().as_ptr(),
                            std::ptr::null(),
                            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                            0,
                            0,
                            w,
                            h,
                            hwnd,
                            0,
                            win32::get_module_handle(),
                            std::ptr::null_mut(),
                        )
                    };
                    if child_hwnd == 0 {
                        unsafe {
                            DestroyWindow(hwnd);
                        }
                        return Err(anyhow::anyhow!(
                            "CreateWindowExW for child container failed"
                        ));
                    }
                    log::warn!(
                        "open_internal: frame={:p} child={:p} size={}x{}",
                        hwnd as *mut c_void,
                        child_hwnd as *mut c_void,
                        w,
                        h
                    );

                    unsafe {
                        MoveWindow(child_hwnd, 0, 0, w, h, 1);
                        ShowWindow(hwnd, SW_SHOW);
                        ShowWindow(child_hwnd, SW_SHOW);
                        UpdateWindow(hwnd);
                        UpdateWindow(child_hwnd);
                    }
                    let owner_dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd) };
                    let child_dpi =
                        unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForWindow(child_hwnd) };
                    let mut owner_rect: windows_sys::Win32::Foundation::RECT =
                        unsafe { std::mem::zeroed() };
                    let mut child_rect: windows_sys::Win32::Foundation::RECT =
                        unsafe { std::mem::zeroed() };
                    unsafe {
                        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(
                            hwnd,
                            &mut owner_rect,
                        );
                        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(
                            child_hwnd,
                            &mut child_rect,
                        );
                    }
                    win32::pump_message_queue(hwnd);

                    log::info!(
                        "open_internal: owner dpi={} rect={}x{} child dpi={} rect={}x{} child={:p}",
                        owner_dpi,
                        owner_rect.right - owner_rect.left,
                        owner_rect.bottom - owner_rect.top,
                        child_dpi,
                        child_rect.right - child_rect.left,
                        child_rect.bottom - child_rect.top,
                        child_hwnd as *mut c_void
                    );
                    let result = win32::plug_view_attached_seh(
                        &plug_view,
                        child_hwnd as *mut c_void,
                        win32::K_PLATFORM_TYPE_HWND,
                    );
                    if result == win32::SEH_CAUGHT_SENTINEL {
                        let (code, addr, rdi, rax, rdx) = win32::seh_get_diagnostics();

                        let mut mod_name = [0u16; 512];
                        let mut mod_handle: windows_sys::Win32::Foundation::HMODULE = 0;
                        unsafe {
                            let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS as u32;
                            if GetModuleHandleExW(flags, addr as *const _, &mut mod_handle) != 0 {
                                GetModuleFileNameW(mod_handle, mod_name.as_mut_ptr(), 512);
                            }
                        }
                        let _mod_str = String::from_utf16_lossy(
                            &mod_name[..mod_name.iter().position(|&c| c == 0).unwrap_or(0)],
                        );
                        let rva = addr as usize - mod_handle as usize;

                        let rdi_dump = win32::seh_dump_rdi(rdi);

                        log::error!(
                            "open_internal: SEH 0x{:08X} rva={:#x} rdi={:#x} rax={:#x} rdx={:#x}",
                            code,
                            rva,
                            rdi,
                            rax,
                            rdx
                        );
                        log::error!(
                            "open_internal: [rdi] = [{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x},{:#x}]",
                            rdi_dump[0],
                            rdi_dump[1],
                            rdi_dump[2],
                            rdi_dump[3],
                            rdi_dump[4],
                            rdi_dump[5],
                            rdi_dump[6],
                            rdi_dump[7],
                            rdi_dump[8],
                            rdi_dump[9],
                            rdi_dump[10],
                            rdi_dump[11],
                            rdi_dump[12],
                            rdi_dump[13],
                            rdi_dump[14],
                            rdi_dump[15]
                        );
                        log::error!(
                            "open_internal: [rdi+0x08]={} [rdi+0x0c]={} [rdi+0x10]={} [rdi+0x14]={}",
                            rdi_dump[2],
                            rdi_dump[3],
                            rdi_dump[4],
                            rdi_dump[5]
                        );
                    }
                    if result != kResultOk {
                        let _ = win32::plug_view_removed_seh(&plug_view);
                        unsafe {
                            DestroyWindow(hwnd);
                        }
                        host_frame::release_host_plug_frame(frame_ptr);
                        let msg = if result == win32::SEH_CAUGHT_SENTINEL {
                            let (code, addr, _, _, _) = win32::seh_get_diagnostics();
                            format!(
                                "IPlugView::attached() crashed (SEH 0x{:08X} at {:p}) in plugin '{}'",
                                code, addr, plugin_name
                            )
                        } else {
                            format!(
                                "IPlugView::attached() failed (result={}) in plugin '{}'",
                                result, plugin_name
                            )
                        };
                        return Err(anyhow::anyhow!("{}", msg));
                    }
                    win32::pump_message_queue(hwnd);
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
                    if !parent.is_null() {
                        host_frame::set_frame_hwnd(frame_ptr, parent as HWND);
                    }
                    let result = win32::plug_view_attached_seh(
                        &plug_view,
                        parent,
                        win32::K_PLATFORM_TYPE_HWND,
                    );
                    if result != kResultOk {
                        let _ = win32::plug_view_removed_seh(&plug_view);
                        host_frame::release_host_plug_frame(frame_ptr);
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
                let _ = win32::plug_view_set_frame_seh(&plug_view, std::ptr::null_mut());
                let _ = win32::plug_view_removed_seh(&plug_view);
            }
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(hwnd) = self.window_hwnd.take() {
                if window_valid && self.owns_window {
                    unsafe {
                        DestroyWindow(hwnd);
                    }
                }
            }
        }
        host_frame::release_host_plug_frame(self.plug_frame);
        self.plug_frame = std::ptr::null_mut();
        self.is_open = false;
        self.owns_window = false;
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

    fn is_window_valid(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            self.window_hwnd.map_or(
                false,
                |hwnd| unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(hwnd) } != 0,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            true
        }
    }

    fn query_view_size(view: &ComPtr<IPlugView>) -> Option<(i32, i32)> {
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if win32::plug_view_get_size_seh(view, &mut rect) == kResultOk {
            return Some((rect.right - rect.left, rect.bottom - rect.top));
        }
        None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst_host::plugin::LoadedPlugin;
    use crate::vst_host::scanner::PluginInfo;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    #[ignore]
    fn smoke_open_plugin_editor() {
        #[cfg(target_os = "windows")]
        unsafe {
            let _ = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        }
        let path = std::env::var("TEST_VST_EDITOR_PATH")
            .ok()
            .map(PathBuf::from);
        let Some(path) = path else {
            return;
        };

        let mut plugin = LoadedPlugin::load(&PluginInfo {
            path,
            name: "EditorSmoke".into(),
            vendor: "".into(),
            category: "".into(),
        })
        .unwrap();
        plugin.setup_processing(44100.0, 256).unwrap();
        let edit_controller = plugin.edit_controller().cloned().unwrap();

        let mut editor = PluginEditor::new();
        editor
            .open_separate_window(&edit_controller, "EditorSmoke", None)
            .unwrap();
        std::thread::sleep(Duration::from_millis(250));
        editor.close();
    }

    #[test]
    #[ignore]
    fn smoke_open_plugin_editor_embedded() {
        #[cfg(target_os = "windows")]
        unsafe {
            let _ = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        }
        let path = std::env::var("TEST_VST_EDITOR_PATH")
            .ok()
            .map(PathBuf::from);
        let Some(path) = path else {
            return;
        };

        #[cfg(target_os = "windows")]
        {
            let mut editor = PluginEditor::new();

            let parent_hwnd = unsafe {
                CreateWindowExW(
                    0,
                    win32::get_editor_class_name().as_ptr(),
                    std::ptr::null(),
                    windows_sys::Win32::UI::WindowsAndMessaging::WS_OVERLAPPEDWINDOW,
                    windows_sys::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
                    windows_sys::Win32::UI::WindowsAndMessaging::CW_USEDEFAULT,
                    800,
                    600,
                    0,
                    0,
                    win32::get_module_handle(),
                    std::ptr::null_mut(),
                )
            };
            assert!(
                parent_hwnd != 0,
                "Failed to create parent window for embedded test"
            );

            let mut plugin = LoadedPlugin::load(&PluginInfo {
                path,
                name: "EditorSmokeEmbedded".into(),
                vendor: "".into(),
                category: "".into(),
            })
            .unwrap();
            plugin.setup_processing(44100.0, 256).unwrap();
            let edit_controller = plugin.edit_controller().cloned().unwrap();

            editor
                .open_embedded_window(
                    &edit_controller,
                    "EditorSmokeEmbedded",
                    parent_hwnd as *mut c_void,
                    (0, 0, 600, 400),
                )
                .unwrap();
            assert!(editor.is_open());
            std::thread::sleep(Duration::from_millis(100));

            editor.close();
            assert!(!editor.is_open());
            std::thread::sleep(Duration::from_millis(50));

            editor
                .open_embedded_window(
                    &edit_controller,
                    "EditorSmokeEmbedded",
                    parent_hwnd as *mut c_void,
                    (0, 0, 600, 400),
                )
                .unwrap();
            assert!(editor.is_open());
            std::thread::sleep(Duration::from_millis(100));

            editor.close();
            assert!(!editor.is_open());

            unsafe {
                DestroyWindow(parent_hwnd);
            }
        }

        #[cfg(not(target_os = "windows"))]
        let _ = path;
    }
}
