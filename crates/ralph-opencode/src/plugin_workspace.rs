//! Opencode workspace inspection and component discovery.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::plugin_catalog::{OpencodeWorkspace, OpencodePluginSource};
use super::plugin_components::{
    OpencodeComponentCounts, OpencodePluginComponentInspector, FilesystemPluginComponentInspector,
};

#[derive(Debug, Clone)]
pub struct OpencodeWorkspaceSummary {
    pub root: PathBuf,
    pub plugin_count: usize,
    pub marketplace_count: usize,
    pub local_counts: Option<OpencodeComponentCounts>,
}

#[derive(Debug, Clone)]
pub struct OpencodeProjectComponentsSummary {
    pub root: PathBuf,
    pub opencode_dir: PathBuf,
    pub counts: OpencodeComponentCounts,
}

#[derive(Debug, Clone, Default)]
pub struct OpencodeFullWorkspaceSummary {
    pub opencode_workspace: Option<OpencodeWorkspaceSummary>,
    pub project_components: Vec<OpencodeProjectComponentsSummary>,
}

pub trait OpencodeWorkspaceInspector: std::fmt::Debug + Send + Sync {
    fn summarize(&self, project_dir: &Path, add_dirs: &[PathBuf]) -> OpencodeFullWorkspaceSummary;
}

#[derive(Debug, Default)]
pub struct FilesystemOpencodeWorkspaceInspector {
    component_inspector: FilesystemPluginComponentInspector,
}

impl FilesystemOpencodeWorkspaceInspector {
    fn summarize_opencode_workspace(&self, project_dir: &Path) -> Option<OpencodeWorkspaceSummary> {
        let workspace = OpencodeWorkspace::discover(project_dir)?;
        let plugin_count = workspace.plugin_catalog().list().len();
        let marketplace_count = workspace.marketplace_len();

        let opencode_dir = workspace.root().join(".opencode");
        let local_counts = if opencode_dir.is_dir() {
            Some(self.component_inspector.inspect_components(&opencode_dir).counts())
        } else {
            None
        };

        Some(OpencodeWorkspaceSummary {
            root: workspace.root().to_path_buf(),
            plugin_count,
            marketplace_count,
            local_counts,
        })
    }

    fn summarize_project_components(
        &self,
        project_dir: &Path,
        add_dirs: &[PathBuf],
    ) -> Vec<OpencodeProjectComponentsSummary> {
        let mut roots = Vec::new();
        roots.push(project_dir.to_path_buf());
        roots.extend(add_dirs.iter().cloned());

        let mut seen = HashSet::new();
        let mut summaries = Vec::new();
        for root in roots {
            if !seen.insert(root.clone()) {
                continue;
            }
            let opencode_dir = root.join(".opencode");
            if !opencode_dir.is_dir() {
                continue;
            }
            let counts = self.component_inspector.inspect_components(&opencode_dir).counts();
            summaries.push(OpencodeProjectComponentsSummary {
                root,
                opencode_dir,
                counts,
            });
        }

        summaries
    }
}

impl OpencodeWorkspaceInspector for FilesystemOpencodeWorkspaceInspector {
    fn summarize(&self, project_dir: &Path, add_dirs: &[PathBuf]) -> OpencodeFullWorkspaceSummary {
        OpencodeFullWorkspaceSummary {
            opencode_workspace: self.summarize_opencode_workspace(project_dir),
            project_components: self.summarize_project_components(project_dir, add_dirs),
        }
    }
}
