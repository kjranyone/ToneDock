use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub name: String,
    pub sample_rate: f64,
    pub buffer_size: u32,
    pub chain: Vec<ChainSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSlot {
    pub plugin_path: String,
    pub plugin_name: String,
    pub enabled: bool,
    pub parameters: Vec<(usize, f32)>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            name: "Untitled".into(),
            sample_rate: 48000.0,
            buffer_size: 256,
            chain: Vec::new(),
        }
    }
}

impl Session {
    pub fn save_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        let session: Session = serde_json::from_str(&json)?;
        Ok(session)
    }
}
