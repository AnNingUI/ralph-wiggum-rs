//! OpenCode plugin catalog and discovery.
//!
//! Discovers plugins from `.opencode/` directory, reads manifests,
//! and provides plugin resolution.

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct OpencodePluginManifest {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpencodePluginDescriptor {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
    pub version: Option<String>,
}

impl OpencodePluginDescriptor {
    pub fn summary(&self) -> String {
        match self
            .version
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(version) => format!("{} v{}", self.name, version),
            None => self.name.clone(),
        }
    }

    pub fn summary_with_description(&self) -> String {
        match self
            .description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(description) => format!("{} - {}", self.summary(), description),
            None => self.summary(),
        }
    }
}

pub trait OpencodePluginSource: std::fmt::Debug + Send + Sync {
    fn resolve(&self, name: &str) -> Option<OpencodePluginDescriptor>;
    fn list(&self) -> Vec<OpencodePluginDescriptor>;
}

#[derive(Debug, Clone)]
pub struct OpencodeWorkspace {
    root: PathBuf,
}

impl OpencodeWorkspace {
    pub fn discover(project_dir: &Path) -> Option<Self> {
        let root = project_dir.join(".opencode");
        if !root.is_dir() {
            return None;
        }
        Some(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn plugin_catalog(&self) -> impl OpencodePluginSource {
        self.clone()
    }

    pub fn marketplace_len(&self) -> usize {
        0
    }

    pub fn plugins_root(&self) -> Option<PathBuf> {
        let path = self.root.join("plugins");
        path.is_dir().then_some(path)
    }

    fn list_plugin_dirs(&self) -> Vec<PathBuf> {
        let Some(plugins_root) = self.plugins_root() else {
            return Vec::new();
        };

        let Ok(entries) = std::fs::read_dir(&plugins_root) else {
            return Vec::new();
        };

        entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect()
    }
}

impl OpencodePluginSource for OpencodeWorkspace {
    fn resolve(&self, name: &str) -> Option<OpencodePluginDescriptor> {
        let plugins_root = self.plugins_root()?;
        let plugin_dir = plugins_root.join(name);

        if !plugin_dir.is_dir() {
            return None;
        }

        let manifest = read_plugin_manifest(&plugin_dir);
        Some(OpencodePluginDescriptor {
            name: manifest
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_else(|| name.to_string()),
            path: plugin_dir,
            description: manifest.as_ref().and_then(|m| m.description.clone()),
            version: manifest.as_ref().and_then(|m| m.version.clone()),
        })
    }

    fn list(&self) -> Vec<OpencodePluginDescriptor> {
        self.list_plugin_dirs()
            .into_iter()
            .filter_map(|plugin_dir| {
                let name = plugin_dir.file_name()?.to_str()?.to_string();
                let manifest = read_plugin_manifest(&plugin_dir);
                Some(OpencodePluginDescriptor {
                    name: manifest.as_ref().map(|m| m.name.clone()).unwrap_or(name),
                    path: plugin_dir,
                    description: manifest.as_ref().and_then(|m| m.description.clone()),
                    version: manifest.as_ref().and_then(|m| m.version.clone()),
                })
            })
            .collect()
    }
}

pub fn plugin_manifest_path(plugin_dir: &Path) -> PathBuf {
    plugin_dir.join("package.json")
}

pub fn read_plugin_manifest(plugin_dir: &Path) -> Option<OpencodePluginManifest> {
    let manifest_path = plugin_manifest_path(plugin_dir);
    let contents = std::fs::read_to_string(manifest_path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn plugin_manifest_name(plugin_dir: &Path) -> Option<String> {
    let manifest = read_plugin_manifest(plugin_dir)?;
    Some(manifest.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_summary_with_version() {
        let desc = OpencodePluginDescriptor {
            name: "test-plugin".to_string(),
            path: PathBuf::from("/test"),
            description: None,
            version: Some("1.0.0".to_string()),
        };
        assert_eq!(desc.summary(), "test-plugin v1.0.0");
    }

    #[test]
    fn descriptor_summary_without_version() {
        let desc = OpencodePluginDescriptor {
            name: "test-plugin".to_string(),
            path: PathBuf::from("/test"),
            description: None,
            version: None,
        };
        assert_eq!(desc.summary(), "test-plugin");
    }

    #[test]
    fn descriptor_summary_with_description() {
        let desc = OpencodePluginDescriptor {
            name: "test-plugin".to_string(),
            path: PathBuf::from("/test"),
            description: Some("A test plugin".to_string()),
            version: Some("1.0.0".to_string()),
        };
        assert_eq!(
            desc.summary_with_description(),
            "test-plugin v1.0.0 - A test plugin"
        );
    }
}
