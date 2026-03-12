//! OpenCode plugin component inspection.
//!
//! Scans plugin directories for commands, scripts, tools, and hooks.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpencodePluginComponentPathSource {
    Manifest,
    Filesystem,
}

#[derive(Debug, Clone)]
pub struct OpencodePluginComponentPath {
    pub path: PathBuf,
    pub exists: bool,
    pub source: OpencodePluginComponentPathSource,
}

#[derive(Debug, Clone, Default)]
pub struct OpencodeComponentCounts {
    pub commands: usize,
    pub scripts: usize,
    pub tools: usize,
}

#[derive(Debug, Clone, Default)]
pub struct OpencodePluginInspection {
    pub commands: Vec<String>,
    pub scripts: Vec<String>,
    pub tools: Vec<String>,
}

impl OpencodePluginInspection {
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

    pub fn has_tool(&self, name: &str) -> bool {
        let needle = name.trim();
        !needle.is_empty()
            && self
                .tools
                .iter()
                .any(|tool| tool.eq_ignore_ascii_case(needle))
    }
}

pub trait OpencodePluginInspector: std::fmt::Debug + Send + Sync {
    fn inspect(&self, plugin_dir: &Path) -> OpencodePluginInspection;
}

#[derive(Debug, Default)]
pub struct FilesystemPluginInspector;

impl OpencodePluginInspector for FilesystemPluginInspector {
    fn inspect(&self, plugin_dir: &Path) -> OpencodePluginInspection {
        OpencodePluginInspection {
            commands: plugin_command_names(plugin_dir),
            scripts: plugin_script_names(plugin_dir),
            tools: plugin_tool_names(plugin_dir),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct OpencodePluginComponents {
    pub command_files: Vec<OpencodePluginComponentPath>,
    pub script_files: Vec<OpencodePluginComponentPath>,
    pub tool_files: Vec<OpencodePluginComponentPath>,
    pub commands: Vec<OpencodePluginCommand>,
    pub scripts: Vec<OpencodePluginScript>,
    pub tools: Vec<OpencodePluginTool>,
}

impl OpencodePluginComponents {
    pub fn counts(&self) -> OpencodeComponentCounts {
        OpencodeComponentCounts {
            commands: self.commands.len(),
            scripts: self.scripts.len(),
            tools: self.tools.len(),
        }
    }
}

pub trait OpencodePluginComponentInspector: std::fmt::Debug + Send + Sync {
    fn inspect_components(&self, plugin_dir: &Path) -> OpencodePluginComponents;
}

#[derive(Debug, Default)]
pub struct FilesystemOpencodePluginInspector;

impl OpencodePluginComponentInspector for FilesystemOpencodePluginInspector {
    fn inspect_components(&self, plugin_dir: &Path) -> OpencodePluginComponents {
        let commands = discover_commands(plugin_dir);
        let scripts = discover_scripts(plugin_dir);
        let tools = discover_tools(plugin_dir);

        let command_files = commands
            .iter()
            .map(|cmd| OpencodePluginComponentPath {
                path: cmd.path.clone(),
                exists: cmd.path.exists(),
                source: OpencodePluginComponentPathSource::Filesystem,
            })
            .collect();

        let script_files = scripts
            .iter()
            .map(|script| OpencodePluginComponentPath {
                path: script.path.clone(),
                exists: script.path.exists(),
                source: OpencodePluginComponentPathSource::Filesystem,
            })
            .collect();

        let tool_files = tools
            .iter()
            .map(|tool| OpencodePluginComponentPath {
                path: tool.path.clone(),
                exists: tool.path.exists(),
                source: OpencodePluginComponentPathSource::Filesystem,
            })
            .collect();

        OpencodePluginComponents {
            command_files,
            script_files,
            tool_files,
            commands,
            scripts,
            tools,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpencodePluginCommand {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct OpencodePluginScript {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct OpencodePluginTool {
    pub name: String,
    pub path: PathBuf,
}

pub fn discover_plugin_components(plugin_dir: &Path) -> OpencodePluginComponents {
    let inspector = FilesystemOpencodePluginInspector;
    inspector.inspect_components(plugin_dir)
}

fn discover_commands(plugin_dir: &Path) -> Vec<OpencodePluginCommand> {
    let commands_dir = plugin_dir.join("commands");
    if !commands_dir.is_dir() {
        return Vec::new();
    }

    list_dir_file_stems(&commands_dir, Some("js"))
        .into_iter()
        .chain(list_dir_file_stems(&commands_dir, Some("ts")))
        .map(|name| OpencodePluginCommand {
            name: name.clone(),
            path: commands_dir.join(format!("{}.js", name)),
        })
        .collect()
}

fn discover_scripts(plugin_dir: &Path) -> Vec<OpencodePluginScript> {
    let scripts_dir = plugin_dir.join("scripts");
    if !scripts_dir.is_dir() {
        return Vec::new();
    }

    list_dir_file_stems(&scripts_dir, Some("js"))
        .into_iter()
        .chain(list_dir_file_stems(&scripts_dir, Some("ts")))
        .map(|name| OpencodePluginScript {
            name: name.clone(),
            path: scripts_dir.join(format!("{}.js", name)),
        })
        .collect()
}

fn discover_tools(plugin_dir: &Path) -> Vec<OpencodePluginTool> {
    let tools_dir = plugin_dir.join("tools");
    if !tools_dir.is_dir() {
        return Vec::new();
    }

    list_dir_file_stems(&tools_dir, Some("js"))
        .into_iter()
        .chain(list_dir_file_stems(&tools_dir, Some("ts")))
        .map(|name| OpencodePluginTool {
            name: name.clone(),
            path: tools_dir.join(format!("{}.js", name)),
        })
        .collect()
}

fn plugin_command_names(plugin_dir: &Path) -> Vec<String> {
    let commands_dir = plugin_dir.join("commands");
    if !commands_dir.is_dir() {
        return Vec::new();
    }

    let mut names = list_dir_file_stems(&commands_dir, Some("js"));
    names.extend(list_dir_file_stems(&commands_dir, Some("ts")));
    names.sort();
    names.dedup();
    names
}

fn plugin_script_names(plugin_dir: &Path) -> Vec<String> {
    let scripts_dir = plugin_dir.join("scripts");
    if !scripts_dir.is_dir() {
        return Vec::new();
    }

    let mut names = list_dir_file_stems(&scripts_dir, Some("js"));
    names.extend(list_dir_file_stems(&scripts_dir, Some("ts")));
    names.sort();
    names.dedup();
    names
}

fn plugin_tool_names(plugin_dir: &Path) -> Vec<String> {
    let tools_dir = plugin_dir.join("tools");
    if !tools_dir.is_dir() {
        return Vec::new();
    }

    let mut names = list_dir_file_stems(&tools_dir, Some("js"));
    names.extend(list_dir_file_stems(&tools_dir, Some("ts")));
    names.sort();
    names.dedup();
    names
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_has_command() {
        let inspection = OpencodePluginInspection {
            commands: vec!["test".to_string(), "build".to_string()],
            scripts: vec![],
            tools: vec![],
        };
        assert!(inspection.has_command("test"));
        assert!(inspection.has_command("TEST"));
        assert!(!inspection.has_command("unknown"));
    }

    #[test]
    fn inspection_has_script() {
        let inspection = OpencodePluginInspection {
            commands: vec![],
            scripts: vec!["deploy".to_string()],
            tools: vec![],
        };
        assert!(inspection.has_script("deploy"));
        assert!(!inspection.has_script("test"));
    }

    #[test]
    fn inspection_has_tool() {
        let inspection = OpencodePluginInspection {
            commands: vec![],
            scripts: vec![],
            tools: vec!["formatter".to_string()],
        };
        assert!(inspection.has_tool("formatter"));
        assert!(!inspection.has_tool("linter"));
    }
}
