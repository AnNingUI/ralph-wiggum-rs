use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
};
use std::io::{self, Stdout};
use std::time::Duration as StdDuration;

use crate::{CodexProgressSnapshot, CodexRenderLine, CodexRenderLineKind};

const MAX_LOG_LINES: usize = 2_000;

#[derive(Debug, Clone)]
pub struct CodexTuiMeta {
    pub model: String,
    pub reasoning_effort: String,
    pub project_path: String,
    pub iteration: u32,
    pub max_iterations: u32,
}

pub struct CodexTui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    meta: CodexTuiMeta,
    logs: Vec<CodexRenderLine>,
    progress: Option<CodexProgressSnapshot>,
    transcript_scroll: usize,
    transcript_follow: bool,
    active_panel: ActivePanel,
    footer: Option<String>,
    restored: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivePanel {
    Transcript,
    Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiInputAction {
    Continue,
    ExitRequested,
}

impl CodexTui {
    pub fn new(meta: CodexTuiMeta) -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let mut ui = Self {
            terminal,
            meta,
            logs: Vec::new(),
            progress: None,
            transcript_scroll: 0,
            transcript_follow: true,
            active_panel: ActivePanel::Transcript,
            footer: None,
            restored: false,
        };
        ui.render()?;
        Ok(ui)
    }

    pub fn push_render_line(&mut self, mut render_line: CodexRenderLine) -> Result<()> {
        render_line.text = render_line.text.replace('\r', "");
        if render_line.text.trim().is_empty() {
            return Ok(());
        }

        if self
            .logs
            .last()
            .is_some_and(|last| last.kind == render_line.kind && last.text == render_line.text)
        {
            return Ok(());
        }

        self.logs.push(render_line);
        if self.logs.len() > MAX_LOG_LINES {
            let extra = self.logs.len() - MAX_LOG_LINES;
            self.logs.drain(0..extra);
        }
        if self.transcript_follow {
            self.transcript_scroll = 0;
        } else {
            self.clamp_transcript_scroll();
        }
        self.render()
    }

    pub fn set_meta(&mut self, meta: CodexTuiMeta) -> Result<()> {
        self.meta = meta;
        self.render()
    }

    pub fn push_stderr(&mut self, text: impl Into<String>) -> Result<()> {
        self.push_render_line(CodexRenderLine {
            kind: CodexRenderLineKind::Error,
            text: text.into(),
        })
    }

    pub fn push_raw_stdout(&mut self, text: impl Into<String>) -> Result<()> {
        self.push_render_line(CodexRenderLine {
            kind: CodexRenderLineKind::Status,
            text: text.into(),
        })
    }

    pub fn set_progress(&mut self, progress: Option<CodexProgressSnapshot>) -> Result<()> {
        if self.progress == progress {
            return Ok(());
        }
        self.progress = progress;
        self.render()
    }

    pub fn set_runtime(
        &mut self,
        progress: Option<CodexProgressSnapshot>,
        footer: Option<String>,
    ) -> Result<()> {
        let changed = self.progress != progress || self.footer != footer;
        if !changed {
            return Ok(());
        }
        self.progress = progress;
        self.footer = footer;
        self.render()
    }

    pub fn handle_input(&mut self) -> Result<TuiInputAction> {
        let mut changed = false;
        while event::poll(StdDuration::from_millis(0))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let is_ctrl_c = matches!(key.code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&'c'))
                && key.modifiers.contains(KeyModifiers::CONTROL);
            if key.code == KeyCode::Esc || is_ctrl_c {
                return Ok(TuiInputAction::ExitRequested);
            }

            match key.code {
                KeyCode::Tab => {
                    self.active_panel = match self.active_panel {
                        ActivePanel::Transcript => ActivePanel::Status,
                        ActivePanel::Status => ActivePanel::Transcript,
                    };
                    changed = true;
                }
                KeyCode::PageUp => {
                    let step = self.transcript_page_size();
                    let max_offset = self.max_transcript_offset();
                    self.transcript_scroll = (self.transcript_scroll + step).min(max_offset);
                    self.transcript_follow = false;
                    changed = true;
                }
                KeyCode::PageDown => {
                    let step = self.transcript_page_size();
                    self.transcript_scroll = self.transcript_scroll.saturating_sub(step);
                    self.transcript_follow = self.transcript_scroll == 0;
                    changed = true;
                }
                KeyCode::Home => {
                    self.transcript_scroll = self.max_transcript_offset();
                    self.transcript_follow = false;
                    changed = true;
                }
                KeyCode::End => {
                    self.transcript_scroll = 0;
                    self.transcript_follow = true;
                    changed = true;
                }
                KeyCode::Up => {
                    let max_offset = self.max_transcript_offset();
                    self.transcript_scroll = (self.transcript_scroll + 1).min(max_offset);
                    self.transcript_follow = false;
                    changed = true;
                }
                KeyCode::Down => {
                    self.transcript_scroll = self.transcript_scroll.saturating_sub(1);
                    self.transcript_follow = self.transcript_scroll == 0;
                    changed = true;
                }
                KeyCode::Char('f') | KeyCode::Char('F') => {
                    self.transcript_scroll = 0;
                    self.transcript_follow = true;
                    changed = true;
                }
                _ => {}
            }
        }

        if changed {
            self.clamp_transcript_scroll();
            self.render()?;
        }
        Ok(TuiInputAction::Continue)
    }

    pub fn set_footer(&mut self, footer: Option<String>) -> Result<()> {
        if self.footer == footer {
            return Ok(());
        }
        self.footer = footer;
        self.render()
    }

    pub fn finish(mut self) -> Result<()> {
        self.restore()
    }

    fn render(&mut self) -> Result<()> {
        let meta = self.meta.clone();
        let footer = self.footer.clone();
        let progress = self.progress.clone();
        let logs = self.logs.clone();
        let active_panel = self.active_panel;
        let transcript_scroll = self.transcript_scroll;
        let transcript_follow = self.transcript_follow;

        self.terminal.draw(|frame| {
            let area = frame.area();
            let root = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(6),
                    Constraint::Min(1),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(area);

            let header_block = Block::default()
                .title(Span::styled(
                    " Codex Session ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL);
            let inner = header_block.inner(root[0]);
            frame.render_widget(header_block, root[0]);

            let header_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ])
                .split(inner);

            let info = Line::from(vec![
                Span::styled("model ", Style::default().fg(Color::DarkGray)),
                Span::styled(meta.model, Style::default().fg(Color::Cyan)),
                Span::raw("  "),
                Span::styled("effort ", Style::default().fg(Color::DarkGray)),
                Span::raw(meta.reasoning_effort),
                Span::raw("  "),
                Span::styled("loop ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{}/{}", meta.iteration, meta.max_iterations)),
            ]);
            frame.render_widget(Paragraph::new(info), header_rows[0]);

            let path_max = area.width.saturating_sub(16).clamp(18, 88) as usize;
            let path_line = Line::from(vec![
                Span::styled("path ", Style::default().fg(Color::DarkGray)),
                Span::raw(shorten_middle(&meta.project_path, path_max)),
            ]);
            frame.render_widget(Paragraph::new(path_line), header_rows[1]);

            let metrics_max = area.width.saturating_sub(18).clamp(24, 120) as usize;
            let metrics = build_metrics_text(progress.as_ref());
            let metrics_line = Line::from(vec![
                Span::styled("state ", Style::default().fg(Color::DarkGray)),
                Span::raw(shorten_middle(&metrics, metrics_max)),
            ]);
            frame.render_widget(Paragraph::new(metrics_line), header_rows[2]);

            let ratio = if meta.max_iterations == 0 {
                0.0
            } else {
                (meta.iteration as f64 / meta.max_iterations as f64).clamp(0.0, 1.0)
            };
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(Color::Cyan))
                .ratio(ratio)
                .label(format!(
                    "iteration {}/{}",
                    meta.iteration, meta.max_iterations
                ));
            frame.render_widget(gauge, header_rows[3]);

            let visible_count = root[1].height.saturating_sub(2) as usize;
            let transcript_lines: Vec<Line> = if logs.is_empty() {
                vec![Line::from(Span::styled(
                    "[status ] waiting for codex output...",
                    Style::default().fg(Color::DarkGray),
                ))]
            } else {
                let view_height = visible_count.max(1);
                let total = logs.len();
                let offset = transcript_scroll.min(total.saturating_sub(1));
                let end = total.saturating_sub(offset);
                let start = end.saturating_sub(view_height);
                logs[start..end]
                    .iter()
                    .into_iter()
                    .map(|log| {
                        let (prefix, prefix_style, body_style) = kind_visual(&log.kind);
                        Line::from(vec![
                            Span::styled(format!("[{prefix:<7}] "), prefix_style),
                            Span::styled(log.text.clone(), body_style),
                        ])
                    })
                    .collect()
            };
            let transcript_title = if transcript_follow {
                " Transcript (follow) ".to_string()
            } else if transcript_scroll == 0 {
                " Transcript ".to_string()
            } else {
                format!(" Transcript +{} ", transcript_scroll)
            };
            let transcript = Paragraph::new(transcript_lines)
                .block(
                    Block::default()
                        .title(transcript_title)
                        .borders(Borders::ALL)
                        .border_style(if active_panel == ActivePanel::Transcript {
                            Style::default().fg(Color::Cyan)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        }),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(transcript, root[1]);

            let footer_present = footer.is_some();
            let status_text = footer.unwrap_or_else(|| "idle".to_string());
            let status_block = Block::default()
                .title(" Status ")
                .borders(Borders::ALL)
                .border_style(if active_panel == ActivePanel::Status {
                    Style::default().fg(Color::Cyan)
                } else if footer_present {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                });
            let status = Paragraph::new(status_text)
                .block(status_block)
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(status, root[2]);

            let hint = Paragraph::new(Line::from(vec![
                Span::styled("ctrl+c", Style::default().fg(Color::Yellow)),
                Span::styled("/esc exit  ", Style::default().fg(Color::DarkGray)),
                Span::styled("up/down", Style::default().fg(Color::Yellow)),
                Span::styled(" line  ", Style::default().fg(Color::DarkGray)),
                Span::styled("pgup/pgdn", Style::default().fg(Color::Yellow)),
                Span::styled(" scroll  ", Style::default().fg(Color::DarkGray)),
                Span::styled("home/end", Style::default().fg(Color::Yellow)),
                Span::styled(" jump  ", Style::default().fg(Color::DarkGray)),
                Span::styled("f", Style::default().fg(Color::Yellow)),
                Span::styled(" follow  ", Style::default().fg(Color::DarkGray)),
                Span::styled("tab", Style::default().fg(Color::Yellow)),
                Span::styled(" switch panel  ", Style::default().fg(Color::DarkGray)),
                Span::styled("--codex-render", Style::default().fg(Color::Yellow)),
                Span::styled(
                    " plain|rich|tui|json-pass|event-json",
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true });
            frame.render_widget(hint, root[3].inner(Margin::new(1, 0)));
        })?;

        Ok(())
    }

    fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }

        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        self.restored = true;
        Ok(())
    }

    fn transcript_page_size(&self) -> usize {
        self.terminal
            .size()
            .map(|size| size.height.saturating_sub(12).max(3) as usize)
            .unwrap_or(12)
    }

    fn max_transcript_offset(&self) -> usize {
        self.logs.len().saturating_sub(1)
    }

    fn clamp_transcript_scroll(&mut self) {
        self.transcript_scroll = self.transcript_scroll.min(self.max_transcript_offset());
    }
}

impl Drop for CodexTui {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn kind_visual(kind: &CodexRenderLineKind) -> (&'static str, Style, Style) {
    match kind {
        CodexRenderLineKind::Assistant => (
            "assistant",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::White),
        ),
        CodexRenderLineKind::Reasoning => (
            "reason",
            Style::default().fg(Color::Blue),
            Style::default().fg(Color::Gray),
        ),
        CodexRenderLineKind::Tool => (
            "tool",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Cyan),
        ),
        CodexRenderLineKind::ToolOutput => (
            "output",
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::Gray),
        ),
        CodexRenderLineKind::Status => (
            "status",
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::DarkGray),
        ),
        CodexRenderLineKind::Error => (
            "error",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Red),
        ),
        CodexRenderLineKind::Todo => (
            "todo",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Yellow),
        ),
    }
}

fn shorten_middle(value: &str, max_chars: usize) -> String {
    let char_count = value.chars().count();
    if char_count <= max_chars || max_chars <= 6 {
        return value.to_string();
    }

    let head = max_chars / 2;
    let tail = max_chars.saturating_sub(head + 3);
    let prefix = value.chars().take(head).collect::<String>();
    let suffix = value
        .chars()
        .rev()
        .take(tail)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("{prefix}...{suffix}")
}

fn build_metrics_text(progress: Option<&CodexProgressSnapshot>) -> String {
    let Some(progress) = progress else {
        return "phase booting".to_string();
    };

    let mut parts = Vec::new();
    parts.push(format!("phase {}", progress.phase));
    parts.push(format!("tools {}", progress.tool_calls));

    if progress.todo_total > 0 {
        parts.push(format!(
            "todo {}/{}",
            progress.todo_completed, progress.todo_total
        ));
    }

    if let Some(last_tool) = progress
        .last_tool
        .as_deref()
        .filter(|tool| !tool.is_empty())
    {
        parts.push(format!("last {last_tool}"));
    }

    if let Some(thread_id) = progress
        .thread_id
        .as_deref()
        .filter(|thread_id| !thread_id.is_empty())
    {
        parts.push(format!("thr {}", short_thread_id(thread_id)));
    }

    if let (Some(input), Some(cached), Some(output)) = (
        progress.input_tokens,
        progress.cached_input_tokens,
        progress.output_tokens,
    ) {
        parts.push(format!(
            "tok i{} c{} o{}",
            compact_token(input),
            compact_token(cached),
            compact_token(output)
        ));
    }

    parts.join(" · ")
}

fn short_thread_id(thread_id: &str) -> String {
    let keep = 6;
    if thread_id.chars().count() <= keep {
        return thread_id.to_string();
    }

    thread_id
        .chars()
        .rev()
        .take(keep)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn compact_token(value: i64) -> String {
    match value {
        1_000_000.. => format!("{:.1}m", value as f64 / 1_000_000.0),
        10_000.. => format!("{:.1}k", value as f64 / 1_000.0),
        1_000.. => format!("{:.0}k", value as f64 / 1_000.0),
        _ => value.to_string(),
    }
}
