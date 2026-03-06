//! State management for Ralph loop

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentModelPair {
    pub agent: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RalphState {
    pub prompt: String,
    pub iteration: u32,
    pub max_iterations: u32,
    pub started_at: DateTime<Utc>,
    pub rotation: Option<Vec<AgentModelPair>>,
    pub rotation_index: Option<usize>,
    pub promise: Option<String>,
    pub tasks_file: Option<String>,
    pub questions_file: Option<String>,
}

impl RalphState {
    pub fn new(prompt: String, max_iterations: u32) -> Self {
        Self {
            prompt,
            iteration: 1,
            max_iterations,
            started_at: Utc::now(),
            rotation: None,
            rotation_index: None,
            promise: None,
            tasks_file: None,
            questions_file: None,
        }
    }

    pub fn get_current_agent_model(&self) -> Option<AgentModelPair> {
        if let (Some(rotation), Some(index)) = (&self.rotation, self.rotation_index) {
            rotation.get(index).cloned()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationHistory {
    pub iteration: u32,
    pub started_at: String,
    pub ended_at: String,
    pub duration_ms: u64,
    pub agent: String,
    pub model: String,
    pub tools_used: HashMap<String, u32>,
    pub files_modified: Vec<String>,
    pub exit_code: i32,
    pub completion_detected: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    pub iterations: Vec<IterationHistory>,
    pub total_duration_ms: u64,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            iterations: Vec::new(),
            total_duration_ms: 0,
        }
    }
}

pub fn get_state_dir() -> PathBuf {
    PathBuf::from(".ralph")
}

pub fn get_state_path() -> PathBuf {
    get_state_dir().join("ralph-loop.state.json")
}

pub fn get_context_path() -> PathBuf {
    get_state_dir().join("ralph-context.md")
}

pub fn get_history_path() -> PathBuf {
    get_state_dir().join("ralph-history.json")
}

pub fn get_tasks_path() -> PathBuf {
    get_state_dir().join("ralph-tasks.md")
}

pub fn get_questions_path() -> PathBuf {
    get_state_dir().join("ralph-questions.json")
}

pub fn ensure_state_dir() -> Result<()> {
    let state_dir = get_state_dir();
    if !state_dir.exists() {
        fs::create_dir_all(&state_dir)?;
    }
    Ok(())
}

pub fn save_state(state: &RalphState) -> Result<()> {
    ensure_state_dir()?;
    let json = serde_json::to_string_pretty(state)?;
    fs::write(get_state_path(), json)?;
    Ok(())
}

pub fn load_state() -> Result<RalphState> {
    let json = fs::read_to_string(get_state_path())?;
    let state = serde_json::from_str(&json)?;
    Ok(state)
}

pub fn state_exists() -> bool {
    get_state_path().exists()
}

pub fn clear_state() -> Result<()> {
    let state_dir = get_state_dir();
    if state_dir.exists() {
        fs::remove_dir_all(&state_dir)?;
    }
    Ok(())
}

pub fn save_history(history: &History) -> Result<()> {
    ensure_state_dir()?;
    let json = serde_json::to_string_pretty(history)?;
    fs::write(get_history_path(), json)?;
    Ok(())
}

pub fn load_history() -> Result<History> {
    let path = get_history_path();
    if !path.exists() {
        return Ok(History::new());
    }
    let json = fs::read_to_string(path)?;
    let history = serde_json::from_str(&json)?;
    Ok(history)
}

pub fn save_context(context: &str) -> Result<()> {
    ensure_state_dir()?;
    fs::write(get_context_path(), context)?;
    Ok(())
}

pub fn load_context() -> Result<Option<String>> {
    let path = get_context_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    Ok(Some(content))
}

pub fn save_tasks(tasks: &str) -> Result<()> {
    ensure_state_dir()?;
    fs::write(get_tasks_path(), tasks)?;
    Ok(())
}

pub fn load_tasks() -> Result<Option<String>> {
    let path = get_tasks_path();
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    Ok(Some(content))
}
