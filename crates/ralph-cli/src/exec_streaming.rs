//! Streaming execution engine for agents that use stream-json format.
//!
//! This module provides a non-blocking execution engine that can handle
//! streaming JSON output without blocking the UI event loop. It uses the
//! ralph-ratatui-ext crate for incremental JSON parsing.

use anyhow::{Result, anyhow};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::signal;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};

use ralph_core::options::AgentOptions;
use ralph_core::plugin::{OutputSink, OutputState, RunContext, Runner};
use ralph_core::progress::ProgressTracker;
use ralph_core::state::{clear_last_message_capture, load_last_message_capture};
use ralph_ratatui_ext::IncrementalJsonParser;

use crate::exec::ExecutionResult;

/// Run one agent invocation with streaming JSON support.
///
/// This is similar to `run_agent_once` but uses incremental JSON parsing
/// to handle stream-json format without blocking the UI.
pub async fn run_agent_streaming(
    runner: &mut dyn Runner,
    prompt: &str,
    model: &str,
    options: &AgentOptions,
    sink: &mut dyn OutputSink,
    progress: &mut ProgressTracker,
    project_dir: &Path,
) -> Result<ExecutionResult> {
    let mut output = OutputState::default();

    // Pre-spawn hook
    {
        let mut ctx = RunContext {
            output: &mut output,
            sink,
            progress,
            project_dir,
        };
        runner.before_spawn(prompt, &mut ctx)?;
    }

    let output_capture_path = runner.output_capture_path();
    if output_capture_path.is_some() {
        clear_last_message_capture()?;
    }

    let prepared_prompt = runner.prepare_prompt(prompt);
    let args = runner.build_args(&prepared_prompt, model, options);
    let env = runner.build_env(options);

    let mut cmd = Command::new(runner.command());
    cmd.args(&args)
        .envs(&env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if runner.wants_stdin_prompt() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    let mut child = cmd.spawn()?;

    // Write prompt to stdin if needed
    if runner.wants_stdin_prompt() {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stdin"))?;
        stdin.write_all(prepared_prompt.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.shutdown().await?;
    }

    let ctrl_c = signal::ctrl_c();
    tokio::pin!(ctrl_c);

    enum StreamMessage {
        StdoutJson(String),
        StdoutRaw(String),
        StderrLine(String),
        StdoutClosed,
        StderrClosed,
    }

    let (tx, mut rx) = mpsc::channel::<StreamMessage>(256);

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Failed to capture stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

    let stdout_tx = tx.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut json_parser = IncrementalJsonParser::new();
        let mut buffer = vec![0u8; 8 * 1024];
        let mut pending = String::with_capacity(8 * 1024);

        loop {
            let read = reader.read(&mut buffer).await;
            match read {
                Ok(0) => {
                    if !pending.is_empty() {
                        pending.push('\n');
                    }
                    break;
                }
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buffer[..n]);
                    pending.push_str(&chunk);
                }
                Err(e) => {
                    let _ = stdout_tx
                        .send(StreamMessage::StderrLine(format!(
                            "Error reading stdout: {e}"
                        )))
                        .await;
                    pending.clear();
                    break;
                }
            }

            while let Some(pos) = pending.find('\n') {
                let mut line: String = pending.drain(..=pos).collect();
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }

                let trimmed = line.trim();
                let payload = if let Some(rest) = trimmed.strip_prefix("data:") {
                    rest.trim_start()
                } else {
                    trimmed
                };

                if payload.is_empty()
                    || payload == "[DONE]"
                    || payload.starts_with("event:")
                    || payload.starts_with("id:")
                {
                    continue;
                }

                let payload_trim = payload.trim_start();
                let looks_json = payload_trim.starts_with('{') || payload_trim.starts_with('[');

                if looks_json {
                    match json_parser.feed(payload_trim) {
                        Ok(objects) => {
                            for obj in objects {
                                let json_str = match serde_json::to_string(&obj) {
                                    Ok(value) => value,
                                    Err(e) => {
                                        let _ = stdout_tx
                                            .send(StreamMessage::StderrLine(format!(
                                                "JSON encode error: {e}"
                                            )))
                                            .await;
                                        continue;
                                    }
                                };
                                if stdout_tx
                                    .send(StreamMessage::StdoutJson(json_str))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = stdout_tx
                                .send(StreamMessage::StderrLine(format!(
                                    "JSON parse error: {e}"
                                )))
                                .await;
                        }
                    }
                } else if stdout_tx
                    .send(StreamMessage::StdoutRaw(payload.to_string()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }

        while let Some(pos) = pending.find('\n') {
            let mut line: String = pending.drain(..=pos).collect();
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            if stdout_tx
                .send(StreamMessage::StdoutRaw(line))
                .await
                .is_err()
            {
                return;
            }
        }

        let _ = stdout_tx.send(StreamMessage::StdoutClosed).await;
    });

    let stderr_tx = tx.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buffer = vec![0u8; 4 * 1024];
        let mut pending = String::with_capacity(4 * 1024);

        loop {
            let read = reader.read(&mut buffer).await;
            match read {
                Ok(0) => {
                    if !pending.is_empty() {
                        pending.push('\n');
                    }
                    break;
                }
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buffer[..n]);
                    pending.push_str(&chunk);
                }
                Err(e) => {
                    let _ = stderr_tx
                        .send(StreamMessage::StderrLine(format!(
                            "Error reading stderr: {e}"
                        )))
                        .await;
                    pending.clear();
                    break;
                }
            }

            while let Some(pos) = pending.find('\n') {
                let mut line: String = pending.drain(..=pos).collect();
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                if stderr_tx
                    .send(StreamMessage::StderrLine(line))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }

        while let Some(pos) = pending.find('\n') {
            let mut line: String = pending.drain(..=pos).collect();
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            if stderr_tx
                .send(StreamMessage::StderrLine(line))
                .await
                .is_err()
            {
                return;
            }
        }

        let _ = stderr_tx.send(StreamMessage::StderrClosed).await;
    });

    // Periodic interrupt polling to keep TUI responsive even when stdout is idle.
    let mut interrupt_tick = interval(std::time::Duration::from_millis(50));
    interrupt_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interrupt_tick.tick().await;

    let mut stdout_closed = false;
    let mut stderr_closed = false;
    let mut process_exited = false;

    // Main event loop
    loop {
        // Check for user interrupt (ESC in TUI, etc.)
        if sink.check_interrupt()? {
            let _ = child.kill().await;
            return Ok(ExecutionResult {
                output_buffer: output.output_buffer,
                tools_used: output.tools_used,
                latest_ai_response: output.latest_ai_response,
                exit_code: -1,
                interrupted: true,
            });
        }

        // Check if process has exited
        if let Ok(Some(_)) = child.try_wait() {
            process_exited = true;
        }

        // If both streams are closed and process exited, we're done
        if stdout_closed && stderr_closed && process_exited {
            break;
        }

        tokio::select! {
            _ = &mut ctrl_c => {
                let _ = child.kill().await;
                return Ok(ExecutionResult {
                    output_buffer: output.output_buffer,
                    tools_used: output.tools_used,
                    latest_ai_response: output.latest_ai_response,
                    exit_code: -1,
                    interrupted: true,
                });
            }

            _ = interrupt_tick.tick() => {
                if sink.check_interrupt()? {
                    let _ = child.kill().await;
                    return Ok(ExecutionResult {
                        output_buffer: output.output_buffer,
                        tools_used: output.tools_used,
                        latest_ai_response: output.latest_ai_response,
                        exit_code: -1,
                        interrupted: true,
                    });
                }
            }

            msg = rx.recv() => {
                let Some(msg) = msg else {
                    stdout_closed = true;
                    stderr_closed = true;
                    continue;
                };
                match msg {
                    StreamMessage::StdoutJson(line) | StreamMessage::StdoutRaw(line) => {
                        let mut ctx = RunContext {
                            output: &mut output,
                            sink,
                            progress,
                            project_dir,
                        };
                        runner.handle_stdout(&line, &mut ctx)?;
                    }
                    StreamMessage::StderrLine(line) => {
                        let mut ctx = RunContext {
                            output: &mut output,
                            sink,
                            progress,
                            project_dir,
                        };
                        runner.handle_stderr(&line, &mut ctx)?;
                    }
                    StreamMessage::StdoutClosed => stdout_closed = true,
                    StreamMessage::StderrClosed => stderr_closed = true,
                }

                if sink.check_interrupt()? {
                    let _ = child.kill().await;
                    return Ok(ExecutionResult {
                        output_buffer: output.output_buffer,
                        tools_used: output.tools_used,
                        latest_ai_response: output.latest_ai_response,
                        exit_code: -1,
                        interrupted: true,
                    });
                }
            }
        }
    }

    let _ = stdout_task.await;
    let _ = stderr_task.await;

    // Finish hook
    {
        let mut ctx = RunContext {
            output: &mut output,
            sink,
            progress,
            project_dir,
        };
        runner.finish(&mut ctx)?;
    }

    // Wait for process to exit and get exit code
    let exit_status = child.wait().await?;
    let exit_code = exit_status.code().unwrap_or(-1);

    // Resolve the latest AI response
    let handler_latest = output
        .latest_ai_response
        .take()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty());

    let captured_latest = if output_capture_path.is_some() {
        load_last_message_capture()?
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
    } else {
        None
    };

    let latest_ai_response =
        runner.resolve_latest_response(exit_code, &output.output_buffer, handler_latest, captured_latest);

    Ok(ExecutionResult {
        output_buffer: output.output_buffer,
        tools_used: output.tools_used,
        latest_ai_response,
        exit_code,
        interrupted: false,
    })
}
