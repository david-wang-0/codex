use super::*;
use pretty_assertions::assert_eq;

#[cfg(unix)]
fn shell_command(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(script);
    command
}

#[cfg(unix)]
#[tokio::test]
async fn command_success_returns_plain_segment() {
    let result = run_fleet_status_command(
        shell_command("printf 'fleet 3 ⚠1 ✉2 ⇄1\\n'"),
        Duration::from_secs(/*secs*/ 1),
    )
    .await;

    assert_eq!(result, Some("fleet 3 ⚠1 ✉2 ⇄1".to_string()));
}

#[tokio::test]
async fn missing_executable_returns_none() {
    let command = Command::new("codex-fleet-status-command-that-does-not-exist");

    assert_eq!(
        run_fleet_status_command(command, Duration::from_secs(/*secs*/ 1)).await,
        None
    );
}

#[cfg(unix)]
#[tokio::test]
async fn nonzero_exit_returns_none() {
    assert_eq!(
        run_fleet_status_command(
            shell_command("printf 'fleet 3'; exit 1"),
            Duration::from_secs(/*secs*/ 1),
        )
        .await,
        None
    );
}

#[cfg(unix)]
#[tokio::test]
async fn command_timeout_returns_none() {
    assert_eq!(
        run_fleet_status_command(
            shell_command("sleep 1"),
            Duration::from_millis(/*millis*/ 20),
        )
        .await,
        None
    );
}

#[cfg(unix)]
#[tokio::test]
async fn oversized_output_returns_none() {
    let oversized = "x".repeat(FLEET_STATUS_MAX_BYTES as usize + 1);
    let script = format!("printf '{oversized}'");

    assert_eq!(
        run_fleet_status_command(shell_command(&script), Duration::from_secs(/*secs*/ 1),).await,
        None
    );
}

#[cfg(unix)]
#[tokio::test]
async fn malformed_output_returns_none() {
    for output in [
        "fleet 3\\nbad",
        "not-fleet 3",
        "fleet 3 ✉2 ⚠1",
        "fleet 3 ⚠0",
        "fleet 3 ✉0",
        "fleet 3 ⇄0",
    ] {
        let script = format!("printf '{output}'");
        assert_eq!(
            run_fleet_status_command(shell_command(&script), Duration::from_secs(/*secs*/ 1),)
                .await,
            None,
            "{output}"
        );
    }
}

#[test]
fn stale_thread_result_does_not_clear_newer_pending_request() {
    let old_thread_id = ThreadId::new();
    let current_thread_id = ThreadId::new();
    let mut state = FleetStatusState {
        active_thread_id: Some(current_thread_id),
        segment: None,
        pending: true,
        last_requested_at: Some(Instant::now()),
    };

    assert!(!state.apply_update(old_thread_id, Some("fleet 9".to_string())));
    assert_eq!(
        state,
        FleetStatusState {
            active_thread_id: Some(current_thread_id),
            segment: None,
            pending: true,
            last_requested_at: state.last_requested_at,
        }
    );
}
