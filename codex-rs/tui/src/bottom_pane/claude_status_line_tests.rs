use super::*;
use pretty_assertions::assert_eq;
use ratatui::style::Color;
use ratatui::style::Modifier;

fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn mirrors_claude_layout_and_compacts_resets() {
    let line = claude_status_line(ClaudeStatusLineData {
        thread_title: Some("Implement the compact status line".to_string()),
        current_dir: "./work/codex".to_string(),
        git_branch: Some("feature/status".to_string()),
        model: "gpt-5.6-sol".to_string(),
        max_context_window: Some(1_000_000),
        context_window: Some(258_000),
        reasoning: "max".to_string(),
        context_used_tokens: Some(134_000),
        five_hour: Some(ClaudeLimit {
            label: "5h",
            used_percent: 49,
            resets_at: Some(10_800),
        }),
        weekly: Some(ClaudeLimit {
            label: "7d",
            used_percent: 85,
            resets_at: Some(180_000),
        }),
        now_epoch_seconds: 0,
    });
    let parts = split_claude_status_line(&line).expect("split status line");

    assert_eq!(
        text(&parts.left),
        "Implement the compact status line: ./work/codex (feature/status)"
    );
    assert_eq!(
        text(&parts.right),
        "gpt-5.6-sol [1M, max]  ctx 134/258k (51%) · 5h 49% (↻3h0m) · 7d 85% (↻2d2h)"
    );
    assert_eq!(
        text(&parts.compact_right),
        "gpt-5.6-sol [1M, max]  ctx 134/258k (51%) · 5h 49% · 7d 85%"
    );
}

#[test]
fn applies_claude_usage_color_bands() {
    let line = right_line(
        &ClaudeStatusLineData {
            thread_title: None,
            current_dir: "~".to_string(),
            git_branch: None,
            model: "gpt".to_string(),
            max_context_window: Some(400_000),
            context_window: Some(100_000),
            reasoning: "high".to_string(),
            context_used_tokens: Some(84_000),
            five_hour: Some(ClaudeLimit {
                label: "5h",
                used_percent: 49,
                resets_at: None,
            }),
            weekly: Some(ClaudeLimit {
                label: "7d",
                used_percent: 85,
                resets_at: None,
            }),
            now_epoch_seconds: 0,
        },
        false,
    );

    assert_eq!(line.spans[3].style.fg, Some(Color::Yellow));
    assert_eq!(line.spans[6].style.fg, Some(Color::LightGreen));
    assert_eq!(line.spans[9].style.fg, Some(Color::Red));
    assert!(line.spans[9].style.add_modifier.contains(Modifier::BOLD));
}
