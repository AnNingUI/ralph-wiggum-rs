//! Agent-agnostic ratatui TUI for ralph-wiggum.
//!
//! Adapted from the Codex-specific `CodexTui` to work with the unified
//! `ProgressSnapshot`, `RenderLine`, and `StatusMeta` types from ralph-core.

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

use ralph_core::progress::ProgressSnapshot;
use ralph_core::render::{RenderKind, RenderLine};
use ralph_core::status::{StatusMeta, shorten_middle};

use crate::status_bar::build_metrics_text;

const MAX_LOG_LINES: usize = 2_000;

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

pub struct RalphTui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    meta: StatusMeta,
    logs: Vec<RenderLine>,
    progress: Option<ProgressSnapshot>,
    transcript_scroll: usize,
    transcript_follow: bool,
    active_panel: ActivePanel,
    footer: Option<String>,
    restored: bool,
}

impl RalphTui {
    pub fn new(meta: StatusMeta) -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Hide)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let mut tui = Self {
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
        tui.draw()?;
        Ok(tui)
    }

    pub fn push_render_line(&mut self, mut render_line: RenderLine) -> Result<()> {
        render_line.text = render_line.text.replace('\r', "");
        if render_line.text.trim().is_empty() {
            return Ok(());
        }

        // Deduplicate consecutive identical lines
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
        self.draw()
    }

    pub fn set_meta(&mut self, meta: StatusMeta) -> Result<()> {
        self.meta = meta;
        self.draw()
    }

    pub fn set_progress(&mut self, progress: Option<ProgressSnapshot>) -> Result<()> {
        if self.progress == progress {
            return Ok(());
        }
        self.progress = progress;
        self.draw()
    }

    pub fn set_runtime(
        &mut self,
        progress: Option<ProgressSnapshot>,
        footer: Option<String>,
    ) -> Result<()> {
        let changed = self.progress != progress || self.footer != footer;
        if !changed {
            return Ok(());
        }
        self.progress = progress;
        self.footer = footer;
        self.draw()
    }

    pub fn set_footer(&mut self, footer: Option<String>) -> Result<()> {
        if self.footer == footer {
            return Ok(());
        }
        self.footer = footer;
        self.draw()
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
            self.draw()?;
        }
        Ok(TuiInputAction::Continue)
    }

    pub fn finish(mut self) -> Result<()> {
        self.restore()
    }

    fn draw(&mut self) -> Result<()> {
        let meta = self.meta.clone();
        let footer = self.footer.clone();
        let progress = self.progress.clone();
        let logs = self.logs.clone();
        let active_panel = self.active_panel;
        let transcript_scroll = self.transcript_scroll;
        let transcript_follow = self.transcript_follow;
        let (loop_iteration, loop_max) = progress
            .as_ref()
            .and_then(
                |snapshot| match (snapshot.loop_iteration, snapshot.loop_max) {
                    (Some(iteration), Some(max)) => Some((iteration, max)),
                    _ => None,
                },
            )
            .unwrap_or((meta.iteration, meta.max_iterations));

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

            // Header panel — agent-agnostic title
            let title = format!(" {} Session ", capitalize(&meta.agent));
            let header_block = Block::default()
                .title(Span::styled(
                    title,
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

            // Row 0: model · effort · loop
            let mut info_spans = vec![
                Span::styled("model ", Style::default().fg(Color::DarkGray)),
                Span::styled(&meta.model, Style::default().fg(Color::Cyan)),
            ];
            if !meta.effort.is_empty() {
                info_spans.push(Span::raw("  "));
                info_spans.push(Span::styled(
                    "effort ",
                    Style::default().fg(Color::DarkGray),
                ));
                info_spans.push(Span::raw(&meta.effort));
            }
            info_spans.push(Span::raw("  "));
            info_spans.push(Span::styled("loop ", Style::default().fg(Color::DarkGray)));
            info_spans.push(Span::raw(format!("{}/{}", loop_iteration, loop_max)));
            frame.render_widget(Paragraph::new(Line::from(info_spans)), header_rows[0]);

            // Row 1: path
            let path_max = area.width.saturating_sub(16).clamp(18, 88) as usize;
            let path_line = Line::from(vec![
                Span::styled("path ", Style::default().fg(Color::DarkGray)),
                Span::raw(shorten_middle(&meta.project_path, path_max)),
            ]);
            frame.render_widget(Paragraph::new(path_line), header_rows[1]);

            // Row 2: metrics from ProgressSnapshot
            let metrics_max = area.width.saturating_sub(18).clamp(24, 120) as usize;
            let metrics = build_metrics_text(progress.as_ref());
            let metrics_line = Line::from(vec![
                Span::styled("state ", Style::default().fg(Color::DarkGray)),
                Span::raw(shorten_middle(&metrics, metrics_max)),
            ]);
            frame.render_widget(Paragraph::new(metrics_line), header_rows[2]);

            // Row 3: iteration gauge
            let ratio = if loop_max == 0 {
                0.0
            } else {
                (loop_iteration as f64 / loop_max as f64).clamp(0.0, 1.0)
            };
            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(Color::Cyan))
                .ratio(ratio)
                .label(format!("iteration {}/{}", loop_iteration, loop_max));
            frame.render_widget(gauge, header_rows[3]);

            // Transcript panel
            let visible_count = root[1].height.saturating_sub(2) as usize;
            let transcript_lines: Vec<Line> = if logs.is_empty() {
                vec![Line::from(Span::styled(
                    "[status ] waiting for agent output...",
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
                    .map(|log| {
                        let (prefix, prefix_style, body_style) = kind_visual(&log.kind);
                        Line::from(vec![
                            Span::styled(format!("[{prefix:<7}] "), prefix_style),
                            Span::styled(&log.text, body_style),
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

            // Status/footer panel
            let footer_present = footer.is_some();
            let status_text = footer.unwrap_or_else(|| "idle".to_string());
            let status_block = Block::default()
                .title(" Status ")
                .borders(Borders::ALL)
                .border_style(if active_panel == ActivePanel::Status || footer_present {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                });
            let status = Paragraph::new(status_text)
                .block(status_block)
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::Gray));
            frame.render_widget(status, root[2]);

            // Hint bar
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
                Span::styled(" switch panel", Style::default().fg(Color::DarkGray)),
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

impl Drop for RalphTui {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Map RenderKind to (prefix, prefix_style, body_style) for the transcript panel.
fn kind_visual(kind: &RenderKind) -> (&'static str, Style, Style) {
    match kind {
        RenderKind::Assistant => (
            "asst",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::White),
        ),
        RenderKind::Reasoning => (
            "reason",
            Style::default().fg(Color::Blue),
            Style::default().fg(Color::Gray),
        ),
        RenderKind::ToolCall => (
            "tool",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Cyan),
        ),
        RenderKind::ToolOutput | RenderKind::ToolOutputDelta => (
            "output",
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::Gray),
        ),
        RenderKind::Approval => (
            "approve",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Magenta),
        ),
        RenderKind::Status | RenderKind::Progress | RenderKind::Subagent => (
            "status",
            Style::default().fg(Color::DarkGray),
            Style::default().fg(Color::DarkGray),
        ),
        RenderKind::Error => (
            "error",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Red),
        ),
        RenderKind::Mcp => (
            "mcp",
            Style::default().fg(Color::Yellow),
            Style::default().fg(Color::Yellow),
        ),
        RenderKind::Todo => (
            "todo",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            Style::default().fg(Color::Yellow),
        ),
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            upper + chars.as_str()
        }
    }
}
