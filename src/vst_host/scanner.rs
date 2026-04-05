use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub path: PathBuf,
    pub name: String,
    pub category: String,
    pub vendor: String,
}

pub struct PluginScanner {
    search_paths: Vec<PathBuf>,
}

impl PluginScanner {
    pub fn new() -> Self {
        Self {
            search_paths: Self::default_vst3_paths(),
        }
    }

    pub fn add_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    pub fn scan(&self) -> Vec<PluginInfo> {
        let mut plugins = Vec::new();
        for search_path in &self.search_paths {
            if search_path.exists() {
                Self::scan_directory(search_path, &mut plugins);
            }
        }
        plugins
    }

    fn scan_directory(dir: &Path, plugins: &mut Vec<PluginInfo>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if Self::is_vst3_bundle(&path) {
                        let name = path
                            .file_stem()
                            .and_then(OsStr::to_str)
                            .unwrap_or("Unknown")
                            .to_string();
                        plugins.push(PluginInfo {
                            path: path.clone(),
                            name,
                            category: String::new(),
                            vendor: String::new(),
                        });
                    } else {
                        Self::scan_directory(&path, plugins);
                    }
                }
            }
        }
    }

    fn is_vst3_bundle(path: &Path) -> bool {
        path.extension()
            .and_then(OsStr::to_str)
            .map(|e| e.eq_ignore_ascii_case("vst3"))
            .unwrap_or(false)
    }

    #[cfg(target_os = "windows")]
    pub fn default_vst3_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            paths.push(PathBuf::from(pf).join("Common Files").join("VST3"));
        }
        if let Some(pf) = std::env::var_os("ProgramFiles(x86)") {
            paths.push(PathBuf::from(pf).join("Common Files").join("VST3"));
        }
        if let Some(lad) = std::env::var_os("LOCALAPPDATA") {
            paths.push(
                PathBuf::from(lad)
                    .join("Programs")
                    .join("Common")
                    .join("VST3"),
            );
        }
        paths
    }

    #[cfg(target_os = "macos")]
    pub fn default_vst3_paths() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/Library/Audio/Plug-Ins/VST3"),
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join("Library")
                .join("Audio")
                .join("Plug-Ins")
                .join("VST3"),
        ]
    }

    #[cfg(target_os = "linux")]
    pub fn default_vst3_paths() -> Vec<PathBuf> {
        let mut paths = vec![
            PathBuf::from("/usr/lib/vst3"),
            PathBuf::from("/usr/local/lib/vst3"),
        ];
        if let Ok(home) = std::env::var("HOME") {
            paths.push(PathBuf::from(home).join(".vst3"));
        }
        paths
    }
}
