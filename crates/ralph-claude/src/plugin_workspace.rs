//! Claude workspace inspection and component discovery.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::plugin_catalog::ClaudeCodeWorkspace;
use super::plugin_components::{
    ClaudeComponentCounts, ClaudePluginComponentInspector, FilesystemPluginComponentInspector,
};

#[derive(Debug, Clone)]
pub struct ClaudeCodeWorkspaceSummary {
    pub root: PathBuf,
    pub plugin_count: usize,
    pub marketplace_count: usize,
    pub local_counts: Option<ClaudeComponentCounts>,
}

#[derive(Debug, Clone)]
pub struct ClaudeProjectComponentsSummary {
    pub root: PathBuf,
    pub claude_dir: PathBuf,
    pub counts: ClaudeComponentCounts,
}

#[derive(Debug, Clone, Default)]
pub struct ClaudeWorkspaceSummary {
    pub claude_code: Option<ClaudeCodeWorkspaceSummary>,
    pub project_components: Vec<ClaudeProjectComponentsSummary>,
}

pub trait ClaudeWorkspaceInspector: std::fmt::Debug + Send + Sync {
    fn summarize(&self, project_dir: &Path, add_dirs: &[PathBuf]) -> ClaudeWorkspaceSummary;
}

#[derive(Debug, Default)]
pub struct FilesystemClaudeWorkspaceInspector {
    component_inspector: FilesystemPluginComponentInspector,
}

impl FilesystemClaudeWorkspaceInspector {
    fn summarize_claude_code(&self, project_dir: &Path) -> Option<ClaudeCodeWorkspaceSummary> {
        let workspace = ClaudeCodeWorkspace::discover(project_dir)?;
        let plugin_count = workspace.plugin_catalog().list().len();
        let marketplace_count = workspace.marketplace_len();

        let claude_dir = workspace.root().join(".claude");
        let local_counts = claude_dir.is_dir().then(|| {
            self.component_inspector
                .inspect_components(&claude_dir)
                .counts()
        });

        Some(ClaudeCodeWorkspaceSummary {
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
    ) -> Vec<ClaudeProjectComponentsSummary> {
        let mut roots = Vec::new();
        roots.push(project_dir.to_path_buf());
        roots.extend(add_dirs.iter().cloned());

        let mut seen = HashSet::new();
        let mut summaries = Vec::new();
        for root in roots {
            if !seen.insert(root.clone()) {
                continue;
            }
            let claude_dir = root.join(".claude");
            if !claude_dir.is_dir() {
                continue;
            }
            let counts = self
                .component_inspector
                .inspect_components(&claude_dir)
                .counts();
            summaries.push(ClaudeProjectComponentsSummary {
                root,
                claude_dir,
                counts,
            });
        }

        summaries
    }
}

impl ClaudeWorkspaceInspector for FilesystemClaudeWorkspaceInspector {
    fn summarize(&self, project_dir: &Path, add_dirs: &[PathBuf]) -> ClaudeWorkspaceSummary {
        ClaudeWorkspaceSummary {
            claude_code: self.summarize_claude_code(project_dir),
            project_components: self.summarize_project_components(project_dir, add_dirs),
        }
    }
}
