use serde::Serialize;

/// Agent-agnostic render line. Shared by TUI and plain output.
#[derive(Debug, Clone, Serialize)]
pub struct RenderLine {
    pub kind: RenderKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderKind {
    Assistant,
    Reasoning,
    ToolCall,
    ToolOutput,
    ToolOutputDelta,
    Approval,
    Status,
    Progress,
    Error,
    Mcp,
    Subagent,
    Todo,
}

/// Output rendering mode, applicable to all agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Plain,
    Rich,
    Tui,
    JsonPass,
    EventJson,
}

impl RenderLine {
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::Assistant,
            text: text.into(),
        }
    }

    pub fn reasoning(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::Reasoning,
            text: text.into(),
        }
    }

    pub fn tool_call(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::ToolCall,
            text: text.into(),
        }
    }

    pub fn tool_output(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::ToolOutput,
            text: text.into(),
        }
    }

    pub fn tool_output_delta(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::ToolOutputDelta,
            text: text.into(),
        }
    }

    pub fn status(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::Status,
            text: text.into(),
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::Error,
            text: text.into(),
        }
    }

    pub fn mcp(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::Mcp,
            text: text.into(),
        }
    }

    pub fn todo(text: impl Into<String>) -> Self {
        Self {
            kind: RenderKind::Todo,
            text: text.into(),
        }
    }
}
