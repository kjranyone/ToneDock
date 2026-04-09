use crate::vst_host::scanner::{PluginInfo, PluginScanner};

#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub id: u32,
    pub name: String,
}

pub struct Chain;

impl Chain {
    pub fn new() -> Self {
        Self
    }

    pub fn scan_plugins(&self) -> anyhow::Result<Vec<PluginInfo>> {
        let scanner = PluginScanner::new();
        Ok(scanner.scan())
    }
}
