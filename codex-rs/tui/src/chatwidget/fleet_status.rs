//! Bounded, host-local fleet status refreshes for the Claude-style status line.

use std::process::Stdio;

use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::*;

const FLEET_STATUS_COMMAND: &str = "agent-fleet-status";
const FLEET_STATUS_MAX_BYTES: u64 = 128;
const FLEET_STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const FLEET_STATUS_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct FleetStatusState {
    active_thread_id: Option<ThreadId>,
    pub(super) segment: Option<String>,
    pending: bool,
    last_requested_at: Option<Instant>,
}

impl FleetStatusState {
    fn reset(&mut self, active_thread_id: Option<ThreadId>) {
        *self = Self {
            active_thread_id,
            ..Self::default()
        };
    }

    fn apply_update(&mut self, thread_id: ThreadId, segment: Option<String>) -> bool {
        if self.active_thread_id != Some(thread_id) {
            return false;
        }
        self.pending = false;
        self.segment = segment;
        true
    }
}

impl ChatWidget {
    pub(super) fn enable_fleet_status_for_current_thread(&mut self) {
        if self.fleet_status.active_thread_id == self.thread_id {
            return;
        }
        self.fleet_status.reset(self.thread_id);
        if self.thread_id.is_some() {
            self.frame_requester.schedule_frame();
        }
    }

    pub(super) fn disable_fleet_status(&mut self) {
        if self.fleet_status.active_thread_id.is_some()
            || self.fleet_status.segment.is_some()
            || self.fleet_status.pending
            || self.fleet_status.last_requested_at.is_some()
        {
            self.fleet_status.reset(/*active_thread_id*/ None);
        }
    }

    pub(super) fn refresh_fleet_status_if_due(&mut self) {
        let Some(thread_id) = self.fleet_status.active_thread_id else {
            return;
        };
        let now = Instant::now();
        let next_due_at = self
            .fleet_status
            .last_requested_at
            .map(|last_requested_at| last_requested_at + FLEET_STATUS_REFRESH_INTERVAL);

        if !self.fleet_status.pending && next_due_at.is_none_or(|due_at| now >= due_at) {
            self.fleet_status.pending = true;
            self.fleet_status.last_requested_at = Some(now);
            let tx = self.app_event_tx.clone();
            tokio::spawn(async move {
                let segment = query_fleet_status(thread_id).await;
                tx.send(AppEvent::StatusLineFleetUpdated { thread_id, segment });
            });
        }

        let delay = self
            .fleet_status
            .last_requested_at
            .map(|last_requested_at| {
                (last_requested_at + FLEET_STATUS_REFRESH_INTERVAL)
                    .saturating_duration_since(Instant::now())
            })
            .unwrap_or(FLEET_STATUS_REFRESH_INTERVAL);
        self.frame_requester.schedule_frame_in(delay);
    }

    pub(crate) fn set_status_line_fleet(
        &mut self,
        thread_id: ThreadId,
        segment: Option<String>,
    ) -> bool {
        self.fleet_status.apply_update(thread_id, segment)
    }
}

async fn query_fleet_status(thread_id: ThreadId) -> Option<String> {
    let mut command = Command::new(FLEET_STATUS_COMMAND);
    command
        .arg("--session")
        .arg(thread_id.to_string())
        .arg("--no-color");
    run_fleet_status_command(command, FLEET_STATUS_TIMEOUT).await
}

async fn run_fleet_status_command(mut command: Command, timeout: Duration) -> Option<String> {
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    tokio::time::timeout(timeout, async move {
        let mut child = command.spawn().ok()?;
        let stdout = child.stdout.take()?;
        let mut bytes = Vec::with_capacity(FLEET_STATUS_MAX_BYTES as usize + 1);
        stdout
            .take(FLEET_STATUS_MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        if bytes.len() as u64 > FLEET_STATUS_MAX_BYTES {
            let _ = child.kill().await;
            return None;
        }
        let status = child.wait().await.ok()?;
        if !status.success() {
            return None;
        }
        parse_command_output(bytes)
    })
    .await
    .ok()
    .flatten()
}

fn parse_command_output(bytes: Vec<u8>) -> Option<String> {
    let output = String::from_utf8(bytes).ok()?;
    let output = output.strip_suffix('\n').unwrap_or(&output);
    let output = output.strip_suffix('\r').unwrap_or(output);
    if output.is_empty()
        || output.chars().any(char::is_control)
        || !crate::bottom_pane::is_canonical_fleet_status(output)
    {
        return None;
    }
    Some(output.to_string())
}

#[cfg(test)]
#[path = "fleet_status_tests.rs"]
mod tests;
