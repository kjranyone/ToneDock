use vst3::Steinberg::Vst::{self, IEditController};
use vst3::Steinberg::kResultOk;
use vst3::ComPtr;

use crate::audio::chain::ParamInfo;

use super::seh_ffi::*;
use super::LoadedPlugin;

impl LoadedPlugin {
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
}
