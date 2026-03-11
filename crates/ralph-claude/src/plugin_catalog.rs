//! Claude Code plugin catalog and discovery.
//!
//! Discovers plugins from `claude-code/` workspace, reads manifests,
//! and provides plugin resolution.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct ClaudePluginManifest {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ClaudePluginMarketplace {
    pub plugins: Vec<ClaudePluginMarketplaceEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaudePluginMarketplaceEntry {
    pub name: String,
    pub source: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClaudePluginDescriptor {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
    pub version: Option<String>,
}

impl ClaudePluginDescriptor {
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

pub trait ClaudePluginSource: std::fmt::Debug + Send + Sync {
    fn resolve(&self, name: &str) -> Option<ClaudePluginDescriptor>;
    fn list(&self) -> Vec<ClaudePluginDescriptor>;
}

#[derive(Debug, Clone)]
pub struct ClaudeCodeWorkspace {
    root: PathBuf,
    marketplace: Option<ClaudePluginMarketplace>,
}

impl ClaudeCodeWorkspace {
    pub fn discover(project_dir: &Path) -> Option<Self> {
        let root = project_dir.join("claude-code");
        if !root.is_dir() {
            return None;
        }
        let marketplace = read_marketplace_manifest(&root);
        Some(Self { root, marketplace })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn marketplace_len(&self) -> usize {
        self.marketplace
            .as_ref()
            .map(|m| m.plugins.len())
            .unwrap_or(0)
    }

    pub fn plugin_catalog(&self) -> PluginCatalog {
        let mut sources: Vec<Box<dyn ClaudePluginSource>> = Vec::new();

        if let Some(marketplace) = &self.marketplace {
            for entry in &marketplace.plugins {
                if let Some(source_path) = entry
                    .source
                    .as_deref()
                    .and_then(|s| resolve_marketplace_source(&self.root, s))
                {
                    sources.push(Box::new(DirectoryPluginSource {
                        root: source_path,
                    }));
                }
            }
        }

        if let Some(plugins_root) = plugins_root(&self.root) {
            sources.push(Box::new(DirectoryPluginSource {
                root: plugins_root,
            }));
        }

        PluginCatalog { sources }
    }
}

#[derive(Debug)]
pub struct PluginCatalog {
    sources: Vec<Box<dyn ClaudePluginSource>>,
}

impl PluginCatalog {
    pub fn resolve(&self, name: &str) -> Option<ClaudePluginDescriptor> {
        for source in &self.sources {
            if let Some(descriptor) = source.resolve(name) {
                return Some(descriptor);
            }
        }
        None
    }

    pub fn list(&self) -> Vec<ClaudePluginDescriptor> {
        let mut seen = HashMap::new();
        let mut result = Vec::new();

        for source in &self.sources {
            for descriptor in source.list() {
                if seen.contains_key(&descriptor.name) {
                    continue;
                }
                seen.insert(descriptor.name.clone(), ());
                result.push(descriptor);
            }
        }

        result
    }
}

#[derive(Debug)]
struct DirectoryPluginSource {
    root: PathBuf,
}

impl ClaudePluginSource for DirectoryPluginSource {
    fn resolve(&self, name: &str) -> Option<ClaudePluginDescriptor> {
        let plugin_dir = self.root.join(name);
        if !plugin_dir.is_dir() {
            return None;
        }
        build_descriptor(&plugin_dir)
    }

    fn list(&self) -> Vec<ClaudePluginDescriptor> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };

        entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if !path.is_dir() {
                    return None;
                }
                build_descriptor(&path)
            })
            .collect()
    }
}

fn build_descriptor(plugin_dir: &Path) -> Option<ClaudePluginDescriptor> {
    let manifest = read_plugin_manifest(plugin_dir);
    let (name, version, description) = if let Some(manifest) = manifest {
        (manifest.name, manifest.version, manifest.description)
    } else {
        let fallback_name = plugin_dir
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.to_string())?;
        (fallback_name, None, None)
    };

    Some(ClaudePluginDescriptor {
        name,
        path: plugin_dir.to_path_buf(),
        description,
        version,
    })
}

pub fn plugin_manifest_path(plugin_dir: &Path) -> PathBuf {
    plugin_dir.join(".claude-plugin").join("manifest.json")
}

pub fn read_plugin_manifest(plugin_dir: &Path) -> Option<ClaudePluginManifest> {
    let manifest_path = plugin_manifest_path(plugin_dir);
    let contents = std::fs::read_to_string(manifest_path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn plugin_manifest_name(plugin_dir: &Path) -> Option<String> {
    let manifest = read_plugin_manifest(plugin_dir)?;
    Some(manifest.name)
}

pub(crate) fn read_plugin_manifest_value(plugin_dir: &Path) -> Option<serde_json::Value> {
    let manifest_path = plugin_manifest_path(plugin_dir);
    let contents = std::fs::read_to_string(manifest_path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn read_marketplace_manifest(root: &Path) -> Option<ClaudePluginMarketplace> {
    let path = root.join(".claude-plugin").join("marketplace.json");
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn resolve_marketplace_source(root: &Path, source: &str) -> Option<PathBuf> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidate = PathBuf::from(trimmed);
    let path = if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    };
    Some(path)
}

fn plugins_root(root: &Path) -> Option<PathBuf> {
    let path = root.join("plugins");
    path.is_dir().then_some(path)
}
