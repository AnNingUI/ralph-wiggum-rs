use serde::Serialize;

/// Agent-agnostic structured event stream.
/// Each Runner parses its native output (Codex JSONL / Claude stream-json) into this type.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    // -- lifecycle --
    SessionStarted {
        session_id: Option<String>,
    },
    TurnStarted {
        turn_id: Option<String>,
    },
    TurnComplete,

    // -- content --
    TextDelta {
        text: String,
        role: Role,
    },
    TextComplete {
        text: String,
        role: Role,
    },
    ReasoningDelta {
        text: String,
    },
    PlanUpdate {
        plan: String,
    },

    // -- tool execution --
    ToolCallBegin {
        call_id: String,
        tool: String,
        detail: Option<String>,
        source: ToolSource,
    },
    ToolCallOutputDelta {
        call_id: String,
        stream: OutputStream,
        chunk: String,
    },
    ToolCallEnd {
        call_id: String,
        tool: String,
        status: ToolStatus,
        duration_ms: Option<u64>,
        exit_code: Option<i32>,
    },

    // -- approval --
    ApprovalRequired {
        id: String,
        command: String,
        detail: Option<String>,
    },
    ApprovalResolved {
        id: String,
        decision: Decision,
    },

    // -- MCP --
    McpServerUpdate {
        server: String,
        status: McpStatus,
    },
    McpStartupComplete {
        ready: Vec<String>,
        failed: Vec<McpFailure>,
    },

    // -- tokens --
    TokenUpdate {
        input: Option<i64>,
        cached: Option<i64>,
        output: Option<i64>,
    },

    // -- subagent --
    SubagentSpawned {
        agent_id: String,
        name: Option<String>,
    },
    SubagentComplete {
        agent_id: String,
    },

    // -- loop control (Claude ralph-plugin) --
    LoopIterationAdvanced {
        iteration: u32,
    },
    LoopOutcome {
        outcome: LoopOutcomeKind,
    },

    // -- context --
    ContextCompacted,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Assistant,
    User,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    Agent,
    User,
    Mcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Completed,
    Failed,
    Declined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approved,
    Rejected,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpStatus {
    Starting,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct McpFailure {
    pub server: String,
    pub error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopOutcomeKind {
    Complete,
    MaxIterations,
    Aborted,
}
