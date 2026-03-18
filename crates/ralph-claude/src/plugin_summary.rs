//! Claude plugin summary generation.

use std::path::{Path, PathBuf};

use super::plugin_catalog::{plugin_manifest_path, read_plugin_manifest};
use super::plugin_components::{
    ClaudePluginComponentInspector, ClaudePluginComponentPathSource, ClaudePluginComponents,
    FilesystemPluginComponentInspector,
};

#[derive(Debug, Clone)]
pub struct ClaudePluginSummary {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub path: PathBuf,
    pub manifest_present: bool,
    pub manifest_valid: bool,
    pub components: ClaudePluginComponents,
    pub missing_hook_files: Vec<PathBuf>,
    pub missing_mcp_files: Vec<PathBuf>,
}

impl ClaudePluginSummary {
    pub fn label(&self) -> String {
        let mut label = match self
            .version
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(version) => format!("{} v{}", self.name, version),
            None => self.name.clone(),
        };
        if let Some(description) = self
            .description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            label = format!("{label} - {description}");
        }
        label
    }
}

pub trait ClaudePluginSummaryProvider: std::fmt::Debug + Send + Sync {
    fn summarize(&self, plugin_dir: &Path) -> ClaudePluginSummary;
}

#[derive(Debug, Default)]
pub struct FilesystemPluginSummaryProvider {
    component_inspector: FilesystemPluginComponentInspector,
}

impl ClaudePluginSummaryProvider for FilesystemPluginSummaryProvider {
    fn summarize(&self, plugin_dir: &Path) -> ClaudePluginSummary {
        let manifest_path = plugin_manifest_path(plugin_dir);
        let manifest_present = manifest_path.is_file();
        let manifest = read_plugin_manifest(plugin_dir);
        let manifest_valid = manifest_present && manifest.is_some();

        let (name, version, description) = if let Some(manifest) = manifest {
            (manifest.name, manifest.version, manifest.description)
        } else {
            let fallback_name = plugin_dir
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string())
                .unwrap_or_else(|| plugin_dir.display().to_string());
            (fallback_name, None, None)
        };

        let components = self.component_inspector.inspect_components(plugin_dir);
        let missing_hook_files = components
            .hook_files
            .iter()
            .filter(|file| !file.exists && file.source == ClaudePluginComponentPathSource::Manifest)
            .map(|file| file.path.clone())
            .collect();
        let missing_mcp_files = components
            .mcp_files
            .iter()
            .filter(|file| !file.exists && file.source == ClaudePluginComponentPathSource::Manifest)
            .map(|file| file.path.clone())
            .collect();

        ClaudePluginSummary {
            name,
            version,
            description,
            path: plugin_dir.to_path_buf(),
            manifest_present,
            manifest_valid,
            components,
            missing_hook_files,
            missing_mcp_files,
        }
    }
}
