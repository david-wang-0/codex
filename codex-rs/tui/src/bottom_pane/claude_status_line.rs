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
    pub(crate) resets_left: Option<i64>,
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
    pub(crate) fleet_status: Option<String>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FleetStatusCounts {
    live: u64,
    approvals: Option<u64>,
    unread: Option<u64>,
    active_runs: Option<u64>,
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
    if let Some(fleet) = data.fleet_status.as_deref().and_then(parse_fleet_status) {
        spans.push(Span::styled("fleet ", gray));
        spans.push(Span::raw(fleet.live.to_string()));
        if let Some(approvals) = fleet.approvals {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("⚠{approvals}"),
                Style::default().red().bold(),
            ));
        }
        if let Some(unread) = fleet.unread {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("✉{unread}"),
                Style::default().yellow(),
            ));
        }
        if let Some(active_runs) = fleet.active_runs {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("⇄{active_runs}"),
                Style::default().cyan(),
            ));
        }
        spans.push(Span::styled(" · ", gray));
    }

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
        if include_resets && let Some(details) = reset_details(limit, data.now_epoch_seconds) {
            spans.push(Span::styled(format!(" ({details})"), gray));
        }
    }
    Line::from(spans)
}

fn parse_fleet_status(segment: &str) -> Option<FleetStatusCounts> {
    let mut parts = segment.split_ascii_whitespace();
    if parts.next()? != "fleet" {
        return None;
    }
    let live = parse_count(parts.next()?)?;
    let mut next = parts.next();
    let approvals = take_optional_count(&mut parts, &mut next, '⚠')?;
    let unread = take_optional_count(&mut parts, &mut next, '✉')?;
    let active_runs = take_optional_count(&mut parts, &mut next, '⇄')?;
    if next.is_some() {
        return None;
    }
    let counts = FleetStatusCounts {
        live,
        approvals,
        unread,
        active_runs,
    };
    let mut canonical = format!("fleet {}", counts.live);
    if let Some(approvals) = counts.approvals {
        canonical.push_str(&format!(" ⚠{approvals}"));
    }
    if let Some(unread) = counts.unread {
        canonical.push_str(&format!(" ✉{unread}"));
    }
    if let Some(active_runs) = counts.active_runs {
        canonical.push_str(&format!(" ⇄{active_runs}"));
    }
    (segment == canonical).then_some(counts)
}

pub(crate) fn is_canonical_fleet_status(segment: &str) -> bool {
    parse_fleet_status(segment).is_some()
}

fn take_optional_count<'a>(
    parts: &mut std::str::SplitAsciiWhitespace<'a>,
    next: &mut Option<&'a str>,
    prefix: char,
) -> Option<Option<u64>> {
    let Some(value) = *next else {
        return Some(None);
    };
    let Some(count) = value.strip_prefix(prefix) else {
        return Some(None);
    };
    let count = parse_count(count)?;
    if count == 0 {
        return None;
    }
    *next = parts.next();
    Some(Some(count))
}

fn parse_count(value: &str) -> Option<u64> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
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

fn reset_details(limit: &ClaudeLimit, now: i64) -> Option<String> {
    let countdown = limit
        .resets_at
        .and_then(|at| reset_countdown(at, now))
        .map(|remaining| format!("↻{remaining}"));
    let count = limit.resets_left.map(|count| {
        let suffix = if count == 1 { "reset" } else { "resets" };
        format!("{count} {suffix} left")
    });
    match (countdown, count) {
        (Some(countdown), Some(count)) => Some(format!("{countdown}; {count}")),
        (Some(countdown), None) => Some(countdown),
        (None, Some(count)) => Some(count),
        (None, None) => None,
    }
}

#[cfg(test)]
#[path = "claude_status_line_tests.rs"]
mod tests;
