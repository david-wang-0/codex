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
        fleet_status: Some("fleet 3 ⚠1 ✉2 ⇄1".to_string()),
        five_hour: Some(ClaudeLimit {
            label: "5h",
            used_percent: 49,
            resets_at: Some(10_800),
            resets_left: None,
        }),
        weekly: Some(ClaudeLimit {
            label: "7d",
            used_percent: 85,
            resets_at: Some(180_000),
            resets_left: Some(1),
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
        "gpt-5.6-sol [1M, max]  fleet 3 ⚠1 ✉2 ⇄1 · ctx 134/258k (51%) · 5h 49% (↻3h0m) · 7d 85% (↻2d2h; 1 reset left)"
    );
    assert_eq!(
        text(&parts.compact_right),
        "gpt-5.6-sol [1M, max]  fleet 3 ⚠1 ✉2 ⇄1 · ctx 134/258k (51%) · 5h 49% · 7d 85%"
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
            fleet_status: None,
            five_hour: Some(ClaudeLimit {
                label: "5h",
                used_percent: 49,
                resets_at: None,
                resets_left: None,
            }),
            weekly: Some(ClaudeLimit {
                label: "7d",
                used_percent: 85,
                resets_at: None,
                resets_left: None,
            }),
            now_epoch_seconds: 0,
        },
        false,
    );

    assert_eq!(
        text(&line),
        "gpt [400k, high]  ctx 84/100k (84%) · 5h 49% · 7d 85%"
    );
    assert_eq!(line.spans[3].style.fg, Some(Color::Yellow));
    assert_eq!(line.spans[6].style.fg, Some(Color::LightGreen));
    assert_eq!(line.spans[9].style.fg, Some(Color::Red));
    assert!(line.spans[9].style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn fleet_status_has_canonical_order_and_semantic_styles() {
    let line = right_line(
        &ClaudeStatusLineData {
            thread_title: None,
            current_dir: "~".to_string(),
            git_branch: None,
            model: "gpt".to_string(),
            max_context_window: None,
            context_window: Some(100_000),
            reasoning: String::new(),
            context_used_tokens: Some(10_000),
            fleet_status: Some("fleet 4 ⚠2 ✉3 ⇄1".to_string()),
            five_hour: None,
            weekly: None,
            now_epoch_seconds: 0,
        },
        /*include_resets*/ false,
    );

    assert_eq!(text(&line), "gpt  fleet 4 ⚠2 ✉3 ⇄1 · ctx 10/100k (10%)");
    assert_eq!(line.spans[2].style.fg, Some(Color::DarkGray));
    assert_eq!(line.spans[3].style.fg, None);
    assert_eq!(line.spans[5].style.fg, Some(Color::Red));
    assert!(line.spans[5].style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(line.spans[7].style.fg, Some(Color::Yellow));
    assert_eq!(line.spans[9].style.fg, Some(Color::Cyan));
    assert_eq!(line.spans[10].style.fg, Some(Color::DarkGray));
}

#[test]
fn malformed_fleet_status_is_omitted() {
    for fleet_status in [
        "fleet",
        "fleet nope",
        "fleet 3 ✉2 ⚠1",
        "fleet 3 ⚠1 ⚠2",
        "fleet 3 unknown",
        "fleet 03",
        "fleet 3 ⚠0",
        "fleet 3 ✉0",
        "fleet 3 ⇄0",
    ] {
        assert_eq!(parse_fleet_status(fleet_status), None, "{fleet_status}");
    }
}

#[test]
fn formats_available_weekly_reset_details() {
    let limit = |resets_at, resets_left| ClaudeLimit {
        label: "7d",
        used_percent: 50,
        resets_at,
        resets_left,
    };

    assert_eq!(
        [
            reset_details(&limit(Some(180_000), Some(1)), 0),
            reset_details(&limit(Some(180_000), Some(2)), 0),
            reset_details(&limit(Some(180_000), None), 0),
            reset_details(&limit(None, Some(0)), 0),
            reset_details(&limit(None, None), 0),
        ],
        [
            Some("↻2d2h; 1 reset left".to_string()),
            Some("↻2d2h; 2 resets left".to_string()),
            Some("↻2d2h".to_string()),
            Some("0 resets left".to_string()),
            None,
        ]
    );
}
