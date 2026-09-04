//! Claude-style, two-sided status-line composition.

use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;

/// Zero-width markers let the existing status-line state carry two-sided layout variants. The
/// footer consumes the markers before rendering, so they are never written to the terminal.
pub(super) const RIGHT_MARKER: &str = "\u{0}codex-status-right\u{0}";
pub(super) const COMPACT_RIGHT_MARKER: &str = "\u{0}codex-status-compact-right\u{0}";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClaudeLimit {
    pub(crate) label: &'static str,
    pub(crate) used_percent: i64,
    pub(crate) resets_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ClaudeStatusLineData {
    pub(crate) thread_title: Option<String>,
    pub(crate) current_dir: String,
    pub(crate) git_branch: Option<String>,
    pub(crate) model: String,
    pub(crate) max_context_window: Option<i64>,
    pub(crate) context_window: Option<i64>,
    pub(crate) reasoning: String,
    pub(crate) context_used_tokens: Option<i64>,
    pub(crate) five_hour: Option<ClaudeLimit>,
    pub(crate) weekly: Option<ClaudeLimit>,
    pub(crate) now_epoch_seconds: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ClaudeStatusLineParts {
    pub(super) left: Line<'static>,
    pub(super) right: Line<'static>,
    pub(super) compact_right: Line<'static>,
}

pub(crate) fn claude_status_line(data: ClaudeStatusLineData) -> Line<'static> {
    let left = left_line(&data);
    let right = right_line(&data, /*include_resets*/ true);
    let compact_right = right_line(&data, /*include_resets*/ false);
    let mut spans = left.spans;
    spans.push(Span::raw(RIGHT_MARKER));
    spans.extend(right.spans);
    spans.push(Span::raw(COMPACT_RIGHT_MARKER));
    spans.extend(compact_right.spans);
    Line::from(spans)
}

pub(super) fn split_claude_status_line(line: &Line<'static>) -> Option<ClaudeStatusLineParts> {
    let right_index = line
        .spans
        .iter()
        .position(|span| span.content == RIGHT_MARKER)?;
    let compact_index = line
        .spans
        .iter()
        .position(|span| span.content == COMPACT_RIGHT_MARKER)?;
    if compact_index <= right_index {
        return None;
    }
    Some(ClaudeStatusLineParts {
        left: Line::from(line.spans[..right_index].to_vec()),
        right: Line::from(line.spans[right_index + 1..compact_index].to_vec()),
        compact_right: Line::from(line.spans[compact_index + 1..].to_vec()),
    })
}

fn left_line(data: &ClaudeStatusLineData) -> Line<'static> {
    let gray = Style::default().fg(Color::DarkGray);
    let mut spans = Vec::new();
    if let Some(title) = data.thread_title.as_deref() {
        spans.push(Span::styled(truncate_title(title), gray));
        spans.push(Span::styled(": ", gray));
    }
    spans.push(Span::styled(data.current_dir.clone(), gray));
    if let Some(branch) = data.git_branch.as_deref() {
        spans.push(Span::styled(format!(" ({branch})"), gray));
    }
    Line::from(spans)
}

fn right_line(data: &ClaudeStatusLineData, include_resets: bool) -> Line<'static> {
    let gray = Style::default().fg(Color::DarkGray);
    let model_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM);
    let mut attrs = Vec::new();
    if let Some(window) = data.max_context_window {
        attrs.push(format_tokens(window));
    }
    if !data.reasoning.is_empty() {
        attrs.push(data.reasoning.clone());
    }
    let attrs = (!attrs.is_empty()).then(|| format!(" [{}]", attrs.join(", ")));
    let mut spans = vec![Span::styled(
        format!("{}{}", data.model, attrs.unwrap_or_default()),
        model_style,
    )];

    spans.push(Span::raw("  "));
    spans.push(Span::styled("ctx ", gray));
    match (data.context_used_tokens, data.context_window) {
        (Some(used), Some(window)) if window > 0 => {
            let percent = ((used.max(0) * 100) / window).clamp(0, 100);
            spans.push(Span::styled(
                format!(
                    "{}/{} ({percent}%)",
                    format_used_tokens(used),
                    format_tokens(window)
                ),
                usage_style(percent),
            ));
        }
        _ => spans.push(Span::styled("--", gray)),
    }

    for limit in [&data.five_hour, &data.weekly].into_iter().flatten() {
        spans.push(Span::styled(" · ", gray));
        spans.push(Span::styled(format!("{} ", limit.label), gray));
        spans.push(Span::styled(
            format!("{}%", limit.used_percent),
            usage_style(limit.used_percent),
        ));
        if include_resets
            && let Some(reset) = limit
                .resets_at
                .and_then(|at| reset_countdown(at, data.now_epoch_seconds))
        {
            spans.push(Span::styled(format!(" (↻{reset})"), gray));
        }
    }
    Line::from(spans)
}

fn truncate_title(title: &str) -> String {
    let mut chars = title.trim().chars();
    let prefix = chars.by_ref().take(39).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn format_tokens(tokens: i64) -> String {
    let tokens = tokens.max(0);
    if tokens >= 1_000_000 && tokens % 1_000_000 == 0 {
        format!("{}M", tokens / 1_000_000)
    } else {
        format!("{}k", (tokens + 500) / 1_000)
    }
}

fn format_used_tokens(tokens: i64) -> String {
    let tokens = tokens.max(0);
    if tokens >= 1_000 {
        ((tokens + 500) / 1_000).to_string()
    } else {
        format!("{:.1}", tokens as f64 / 1_000.0)
    }
}

fn usage_style(percent: i64) -> Style {
    match percent {
        85.. => Style::default().red().bold(),
        50..=84 => Style::default().yellow(),
        _ => Style::default().light_green(),
    }
}

fn reset_countdown(resets_at: i64, now: i64) -> Option<String> {
    let remaining = resets_at.checked_sub(now)?;
    if remaining <= 0 {
        return None;
    }
    let days = remaining / 86_400;
    let hours = remaining % 86_400 / 3_600;
    let minutes = remaining % 3_600 / 60;
    if days > 0 {
        Some(format!("{days}d{hours}h"))
    } else if hours > 0 {
        Some(format!("{hours}h{minutes}m"))
    } else {
        Some(format!("{minutes}m"))
    }
}

#[cfg(test)]
#[path = "claude_status_line_tests.rs"]
mod tests;
