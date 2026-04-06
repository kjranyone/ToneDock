use crate::vst_host::plugin::LoadedPlugin;
use crate::vst_host::scanner::{PluginInfo, PluginScanner};

#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
}

pub struct PluginSlot {
    pub info: PluginInfo,
    pub instance: Option<LoadedPlugin>,
    pub enabled: bool,
    pub bypassed: bool,
}

pub struct Chain {
    pub slots: Vec<PluginSlot>,
}

impl Chain {
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    pub fn add_plugin(
        &mut self,
        info: &PluginInfo,
        sample_rate: f64,
        block_size: i32,
    ) -> anyhow::Result<()> {
        log::info!(
            "Chain::add_plugin start: {} (sr={}, bs={})",
            info.name,
            sample_rate,
            block_size
        );
        let mut plugin = LoadedPlugin::load(info)?;
        log::info!("Chain: LoadedPlugin::load succeeded for {}", info.name);
        plugin.setup_processing(sample_rate, block_size)?;
        log::info!("Chain: setup_processing succeeded for {}", info.name);
        let slot = PluginSlot {
            info: info.clone(),
            instance: Some(plugin),
            enabled: true,
            bypassed: false,
        };
        self.slots.push(slot);
        log::info!(
            "Chain: plugin {} added to chain (total slots: {})",
            info.name,
            self.slots.len()
        );
        Ok(())
    }

    pub fn remove_plugin(&mut self, index: usize) {
        if index < self.slots.len() {
            self.slots.remove(index);
        }
    }

    pub fn move_plugin(&mut self, from: usize, to: usize) {
        if from < self.slots.len() && to < self.slots.len() && from != to {
            let slot = self.slots.remove(from);
            self.slots.insert(to, slot);
        }
    }

    #[allow(dead_code)]
    pub fn process(&mut self, input: &[&[f32]], output: &mut [&mut [f32]], num_frames: usize) {
        for ch in 0..output.len() {
            let in_ch = input.get(ch).copied().unwrap_or(&[0.0f32; 0]);
            let len = num_frames.min(output[ch].len()).min(in_ch.len());
            output[ch][..len].copy_from_slice(&in_ch[..len]);
        }

        for slot in &mut self.slots {
            if !slot.enabled || slot.bypassed {
                continue;
            }
            if let Some(ref mut plugin) = slot.instance {
                plugin.process_in_place(output, num_frames as i32);
            }
        }
    }

    pub fn scan_plugins(&self) -> anyhow::Result<Vec<PluginInfo>> {
        let scanner = PluginScanner::new();
        Ok(scanner.scan())
    }

    pub fn get_parameter_info(&self, slot_index: usize) -> Vec<ParamInfo> {
        if let Some(slot) = self.slots.get(slot_index) {
            if let Some(ref plugin) = slot.instance {
                return plugin.parameter_info();
            }
        }
        Vec::new()
    }

    pub fn get_parameter(&self, slot_index: usize, param_index: usize) -> Option<f32> {
        let slot = self.slots.get(slot_index)?;
        let plugin = slot.instance.as_ref()?;
        Some(plugin.get_parameter(param_index))
    }

    pub fn set_parameter(&mut self, slot_index: usize, param_index: usize, value: f32) {
        if let Some(slot) = self.slots.get_mut(slot_index) {
            if let Some(ref mut plugin) = slot.instance {
                plugin.set_parameter(param_index, value);
            }
        }
    }
}
