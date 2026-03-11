//! Claude plugin component inspection.
//!
//! Scans plugin directories for commands, agents, skills, hooks, MCP servers.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use super::plugin_catalog::read_plugin_manifest_value;

#[derive(Debug, Clone, Default)]
pub struct ClaudePluginInspection {
    pub commands: Vec<String>,
    pub scripts: Vec<String>,
}

impl ClaudePluginInspection {
    pub fn has_command(&self, name: &str) -> bool {
        let needle = name.trim();
        !needle.is_empty()
            && self
                .commands
                .iter()
                .any(|command| command.eq_ignore_ascii_case(needle))
    }

    pub fn has_script(&self, name: &str) -> bool {
        let needle = name.trim();
        !needle.is_empty()
            && self
                .scripts
                .iter()
                .any(|script| script.eq_ignore_ascii_case(needle))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudePluginComponentPathSource {
    Default,
    Manifest,
}

impl ClaudePluginComponentPathSource {
    fn merge(self, other: Self) -> Self {
        if matches!(self, Self::Manifest) || matches!(other, Self::Manifest) {
            Self::Manifest
        } else {
            Self::Default
        }
    }
}

#[derive(Debug, Clone)]
struct ClaudePluginComponentPath {
    path: PathBuf,
    source: ClaudePluginComponentPathSource,
}

pub trait ClaudePluginInspector: std::fmt::Debug + Send + Sync {
    fn inspect(&self, plugin_dir: &Path) -> ClaudePluginInspection;
}

#[derive(Debug, Default)]
pub struct FilesystemPluginInspector;

impl ClaudePluginInspector for FilesystemPluginInspector {
    fn inspect(&self, plugin_dir: &Path) -> ClaudePluginInspection {
        ClaudePluginInspection {
            commands: plugin_command_names(plugin_dir),
            scripts: plugin_script_names(plugin_dir),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClaudePluginComponents {
    pub commands: Vec<ClaudePluginCommand>,
    pub agents: Vec<ClaudePluginAgent>,
    pub skills: Vec<ClaudePluginSkill>,
    pub hook_files: Vec<ClaudePluginHookFile>,
    pub hooks: Vec<ClaudePluginHook>,
    pub mcp_files: Vec<ClaudePluginMcpFile>,
    pub mcp_servers: Vec<ClaudePluginMcpServer>,
}

impl ClaudePluginComponents {
    pub fn counts(&self) -> ClaudeComponentCounts {
        ClaudeComponentCounts {
            commands: self.commands.len(),
            agents: self.agents.len(),
            skills: self.skills.len(),
            hooks: self.hooks.len(),
            mcp_servers: self.mcp_servers.len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClaudePluginCommand {
    pub name: String,
    pub path: PathBuf,
    pub exists: bool,
    pub source: ClaudePluginComponentPathSource,
}

#[derive(Debug, Clone)]
pub struct ClaudePluginAgent {
    pub name: String,
    pub path: PathBuf,
    pub exists: bool,
    pub source: ClaudePluginComponentPathSource,
}

#[derive(Debug, Clone)]
pub struct ClaudePluginSkill {
    pub name: String,
    pub path: PathBuf,
    pub exists: bool,
    pub source: ClaudePluginComponentPathSource,
}

#[derive(Debug, Clone)]
pub struct ClaudePluginHookFile {
    pub path: PathBuf,
    pub exists: bool,
    pub source: ClaudePluginComponentPathSource,
}

#[derive(Debug, Clone)]
pub struct ClaudePluginHook {
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone)]
pub struct ClaudePluginMcpFile {
    pub path: PathBuf,
    pub exists: bool,
    pub source: ClaudePluginComponentPathSource,
}

#[derive(Debug, Clone)]
pub struct ClaudePluginMcpServer {
    pub name: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClaudeComponentCounts {
    pub commands: usize,
    pub agents: usize,
    pub skills: usize,
    pub hooks: usize,
    pub mcp_servers: usize,
}

pub trait ClaudePluginComponentInspector: std::fmt::Debug + Send + Sync {
    fn inspect_components(&self, plugin_dir: &Path) -> ClaudePluginComponents;
}

#[derive(Debug, Default)]
pub struct FilesystemPluginComponentInspector;

impl ClaudePluginComponentInspector for FilesystemPluginComponentInspector {
    fn inspect_components(&self, plugin_dir: &Path) -> ClaudePluginComponents {
        let commands = inspect_commands(plugin_dir);
        let agents = inspect_agents(plugin_dir);
        let skills = inspect_skills(plugin_dir);
        let hook_files = inspect_hook_files(plugin_dir);
        let hooks = inspect_hooks(plugin_dir);
        let mcp_files = inspect_mcp_files(plugin_dir);
        let mcp_servers = inspect_mcp_servers(plugin_dir);

        ClaudePluginComponents {
            commands,
            agents,
            skills,
            hook_files,
            hooks,
            mcp_files,
            mcp_servers,
        }
    }
}

fn inspect_commands(plugin_dir: &Path) -> Vec<ClaudePluginCommand> {
    let mut paths = collect_component_paths(plugin_dir, "commands", None);
    paths.extend(collect_manifest_component_paths(
        plugin_dir,
        &["commands"],
        None,
    ));

    let mut seen = HashMap::new();
    let mut result = Vec::new();

    for component_path in paths {
        let Some(name) = component_path
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.to_string())
        else {
            continue;
        };

        let entry = seen.entry(name.clone()).or_insert(ClaudePluginCommand {
            name: name.clone(),
            path: component_path.path.clone(),
            exists: component_path.path.is_file(),
            source: component_path.source,
        });

        entry.source = entry.source.merge(component_path.source);
        if component_path.path.is_file() {
            entry.exists = true;
            entry.path = component_path.path;
        }
    }

    result.extend(seen.into_values());
    result
}

fn inspect_agents(plugin_dir: &Path) -> Vec<ClaudePluginAgent> {
    let mut paths = collect_component_paths(plugin_dir, "agents", Some("json"));
    paths.extend(collect_manifest_component_paths(
        plugin_dir,
        &["agents"],
        Some("json"),
    ));

    let mut seen = HashMap::new();
    let mut result = Vec::new();

    for component_path in paths {
        let Some(name) = component_path
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.to_string())
        else {
            continue;
        };

        let entry = seen.entry(name.clone()).or_insert(ClaudePluginAgent {
            name: name.clone(),
            path: component_path.path.clone(),
            exists: component_path.path.is_file(),
            source: component_path.source,
        });

        entry.source = entry.source.merge(component_path.source);
        if component_path.path.is_file() {
            entry.exists = true;
            entry.path = component_path.path;
        }
    }

    result.extend(seen.into_values());
    result
}

fn inspect_skills(plugin_dir: &Path) -> Vec<ClaudePluginSkill> {
    let mut paths = collect_component_paths(plugin_dir, "skills", Some("md"));
    paths.extend(collect_manifest_component_paths(
        plugin_dir,
        &["skills"],
        Some("md"),
    ));

    let mut seen = HashMap::new();
    let mut result = Vec::new();

    for component_path in paths {
        let Some(name) = component_path
            .path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.to_string())
        else {
            continue;
        };

        let entry = seen.entry(name.clone()).or_insert(ClaudePluginSkill {
            name: name.clone(),
            path: component_path.path.clone(),
            exists: component_path.path.is_file(),
            source: component_path.source,
        });

        entry.source = entry.source.merge(component_path.source);
        if component_path.path.is_file() {
            entry.exists = true;
            entry.path = component_path.path;
        }
    }

    result.extend(seen.into_values());
    result
}

fn inspect_hook_files(plugin_dir: &Path) -> Vec<ClaudePluginHookFile> {
    let mut paths = collect_component_paths(plugin_dir, "hooks", None);
    paths.extend(collect_manifest_component_paths(
        plugin_dir,
        &["hooks"],
        None,
    ));

    let mut seen = HashMap::new();
    let mut result = Vec::new();

    for component_path in paths {
        let key = component_path.path.clone();
        let entry = seen.entry(key.clone()).or_insert(ClaudePluginHookFile {
            path: component_path.path.clone(),
            exists: component_path.path.is_file(),
            source: component_path.source,
        });

        entry.source = entry.source.merge(component_path.source);
        if component_path.path.is_file() {
            entry.exists = true;
        }
    }

    result.extend(seen.into_values());
    result
}

fn inspect_hooks(plugin_dir: &Path) -> Vec<ClaudePluginHook> {
    let manifest_value = read_plugin_manifest_value(plugin_dir);
    let Some(manifest_value) = manifest_value else {
        return Vec::new();
    };

    let Some(hooks_obj) = manifest_value
        .get("hooks")
        .and_then(|value| value.as_object())
    else {
        return Vec::new();
    };

    hooks_obj
        .iter()
        .filter_map(|(name, value)| {
            let command = value.as_str()?.to_string();
            Some(ClaudePluginHook {
                name: name.clone(),
                command,
            })
        })
        .collect()
}

fn inspect_mcp_files(plugin_dir: &Path) -> Vec<ClaudePluginMcpFile> {
    let mut paths = collect_component_paths(plugin_dir, "mcp", Some("json"));
    paths.extend(collect_manifest_component_paths(
        plugin_dir,
        &["mcp"],
        Some("json"),
    ));

    let mut seen = HashMap::new();
    let mut result = Vec::new();

    for component_path in paths {
        let key = component_path.path.clone();
        let entry = seen.entry(key.clone()).or_insert(ClaudePluginMcpFile {
            path: component_path.path.clone(),
            exists: component_path.path.is_file(),
            source: component_path.source,
        });

        entry.source = entry.source.merge(component_path.source);
        if component_path.path.is_file() {
            entry.exists = true;
        }
    }

    result.extend(seen.into_values());
    result
}

fn inspect_mcp_servers(plugin_dir: &Path) -> Vec<ClaudePluginMcpServer> {
    let manifest_value = read_plugin_manifest_value(plugin_dir);
    let Some(manifest_value) = manifest_value else {
        return Vec::new();
    };

    let Some(mcp_obj) = manifest_value.get("mcp").and_then(|value| value.as_object()) else {
        return Vec::new();
    };

    mcp_obj
        .keys()
        .map(|name| ClaudePluginMcpServer {
            name: name.clone(),
        })
        .collect()
}

fn collect_component_paths(
    plugin_dir: &Path,
    subdir: &str,
    extension: Option<&str>,
) -> Vec<ClaudePluginComponentPath> {
    let dir = plugin_dir.join(subdir);
    if !dir.is_dir() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            if let Some(ext) = extension {
                let path_ext = path.extension()?.to_str()?;
                if !path_ext.eq_ignore_ascii_case(ext) {
                    return None;
                }
            }
            Some(ClaudePluginComponentPath {
                path,
                source: ClaudePluginComponentPathSource::Default,
            })
        })
        .collect()
}

fn collect_manifest_component_paths(
    plugin_dir: &Path,
    json_path: &[&str],
    extension: Option<&str>,
) -> Vec<ClaudePluginComponentPath> {
    let manifest_value = read_plugin_manifest_value(plugin_dir);
    let Some(manifest_value) = manifest_value else {
        return Vec::new();
    };

    let mut current = &manifest_value;
    for segment in json_path {
        let Some(next) = current.get(segment) else {
            return Vec::new();
        };
        current = next;
    }

    let Some(arr) = current.as_array() else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|value| {
            let path_str = value.as_str()?;
            let path = resolve_component_path(plugin_dir, path_str)?;
            if let Some(ext) = extension {
                let path_ext = path.extension()?.to_str()?;
                if !path_ext.eq_ignore_ascii_case(ext) {
                    return None;
                }
            }
            Some(ClaudePluginComponentPath {
                path,
                source: ClaudePluginComponentPathSource::Manifest,
            })
        })
        .collect()
}

fn resolve_component_path(plugin_dir: &Path, path_str: &str) -> Option<PathBuf> {
    let trimmed = path_str.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        return Some(candidate);
    }

    let mut components = candidate.components();
    let first = components.next()?;

    if matches!(first, Component::ParentDir) {
        return None;
    }

    Some(plugin_dir.join(candidate))
}

pub fn resolve_hook_command_path(plugin_dir: &Path, command: &str) -> Option<PathBuf> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    let first_part = parts[0];
    let candidate = PathBuf::from(first_part);

    if candidate.is_absolute() {
        return Some(candidate);
    }

    let mut components = candidate.components();
    let first_component = components.next()?;

    if matches!(first_component, Component::ParentDir) {
        return None;
    }

    Some(plugin_dir.join(candidate))
}

fn plugin_command_names(plugin_dir: &Path) -> Vec<String> {
    list_dir_file_stems(&plugin_dir.join("commands"), None)
}

fn plugin_script_names(plugin_dir: &Path) -> Vec<String> {
    list_dir_file_names(&plugin_dir.join("scripts"))
}

fn list_dir_file_stems(dir: &Path, extension: Option<&str>) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            if let Some(ext) = extension {
                let path_ext = path.extension()?.to_str()?;
                if !path_ext.eq_ignore_ascii_case(ext) {
                    return None;
                }
            }
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.to_string())
        })
        .collect()
}

fn list_dir_file_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .collect()
}
