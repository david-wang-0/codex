use std::ffi::OsStr;
use std::ffi::OsString;
use std::sync::Arc;

use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use codex_protocol::SessionId;
use codex_protocol::ThreadId;
use codex_protocol::shell_environment::CODEX_PARENT_THREAD_ID_ENV_VAR;
use codex_protocol::shell_environment::CODEX_SESSION_ID_ENV_VAR;
use codex_protocol::shell_environment::CODEX_THREAD_ID_ENV_VAR;
use codex_protocol::shell_environment::CODEX_THREAD_TOKEN_ENV_VAR;
use codex_protocol::shell_environment::ThreadToken;
use codex_utils_absolute_path::AbsolutePathBuf;
use futures::future::BoxFuture;
use serde::Serialize;
use serde::Serializer;

pub type HookFn = Arc<dyn for<'a> Fn(&'a HookPayload) -> BoxFuture<'a, HookResult> + Send + Sync>;

/// Identity of the Codex thread whose hooks are being run.
///
/// Hook child processes (command hooks and legacy `notify`) receive these IDs
/// as reserved environment variables (`CODEX_THREAD_ID`,
/// `CODEX_PARENT_THREAD_ID`, `CODEX_SESSION_ID`, `CODEX_THREAD_TOKEN`), applied
/// after any handler-configured environment so hook configuration cannot spoof
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookSessionIdentity {
    /// The thread that owns the hooks.
    pub thread_id: ThreadId,
    /// The immediate native parent thread; `None` for a root thread.
    pub parent_thread_id: Option<ThreadId>,
    /// The identity shared by the root thread and all of its descendants.
    pub session_id: SessionId,
    /// This thread's secret, proving to a local service that a hook process
    /// was launched by this thread.
    pub thread_token: ThreadToken,
}

impl HookSessionIdentity {
    /// Identity of a root thread, whose session ID is its own thread ID, with a
    /// freshly generated thread token.
    pub fn root(thread_id: ThreadId) -> Self {
        Self {
            thread_id,
            parent_thread_id: None,
            session_id: SessionId::from(thread_id),
            thread_token: ThreadToken::generate(),
        }
    }

    /// Reserved environment variables exported to hook processes.
    ///
    /// `CODEX_PARENT_THREAD_ID` is only present for a child thread.
    /// `CODEX_THREAD_TOKEN` carries the secret and must not be logged.
    pub fn reserved_environment(&self) -> Vec<(OsString, OsString)> {
        let mut reserved = vec![(
            OsString::from(CODEX_THREAD_ID_ENV_VAR),
            OsString::from(self.thread_id.to_string()),
        )];
        if let Some(parent_thread_id) = self.parent_thread_id {
            reserved.push((
                OsString::from(CODEX_PARENT_THREAD_ID_ENV_VAR),
                OsString::from(parent_thread_id.to_string()),
            ));
        }
        reserved.push((
            OsString::from(CODEX_SESSION_ID_ENV_VAR),
            OsString::from(self.session_id.to_string()),
        ));
        reserved.push((
            OsString::from(CODEX_THREAD_TOKEN_ENV_VAR),
            OsString::from(self.thread_token.expose_for_child_process_env()),
        ));
        reserved
    }

    /// Replaces any ambient or configured value of a reserved identity
    /// variable in `environment` with this identity's values.
    ///
    /// All three names are removed even when the identity exports nothing for
    /// them (a root has no parent), so a stale `CODEX_PARENT_THREAD_ID` cannot
    /// leak through from the captured process environment.
    pub fn apply_to_environment(&self, environment: &mut Vec<(OsString, OsString)>) {
        environment.retain(|(name, _)| !is_reserved_identity_env_var(name));
        environment.extend(self.reserved_environment());
    }
}

/// Whether `name` is one of the reserved thread identity environment variables.
pub(crate) fn is_reserved_identity_env_var(name: &OsStr) -> bool {
    [
        CODEX_THREAD_ID_ENV_VAR,
        CODEX_PARENT_THREAD_ID_ENV_VAR,
        CODEX_SESSION_ID_ENV_VAR,
        CODEX_THREAD_TOKEN_ENV_VAR,
    ]
    .iter()
    .any(|reserved| {
        #[cfg(windows)]
        {
            name.to_string_lossy().eq_ignore_ascii_case(reserved)
        }
        #[cfg(not(windows))]
        {
            name == OsStr::new(reserved)
        }
    })
}

#[derive(Debug)]
pub enum HookResult {
    /// Success: hook completed successfully.
    Success,
    /// FailedContinue: hook failed, but other subsequent hooks should still execute and the
    /// operation should continue.
    FailedContinue(Box<dyn std::error::Error + Send + Sync + 'static>),
    /// FailedAbort: hook failed, other subsequent hooks should not execute, and the operation
    /// should be aborted.
    FailedAbort(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl HookResult {
    pub fn should_abort_operation(&self) -> bool {
        matches!(self, Self::FailedAbort(_))
    }
}

#[derive(Debug)]
pub struct HookResponse {
    pub hook_name: String,
    pub result: HookResult,
}

#[derive(Clone)]
pub struct Hook {
    pub name: String,
    pub func: HookFn,
}

impl Default for Hook {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            func: Arc::new(|_| Box::pin(async { HookResult::Success })),
        }
    }
}

impl Hook {
    pub async fn execute(&self, payload: &HookPayload) -> HookResponse {
        HookResponse {
            hook_name: self.name.clone(),
            result: (self.func)(payload).await,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct HookPayload {
    pub session_id: ThreadId,
    pub cwd: AbsolutePathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
    #[serde(serialize_with = "serialize_triggered_at")]
    pub triggered_at: DateTime<Utc>,
    pub hook_event: HookEvent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HookEventAfterAgent {
    pub thread_id: ThreadId,
    pub turn_id: String,
    pub input_messages: Vec<String>,
    pub last_assistant_message: Option<String>,
}

fn serialize_triggered_at<S>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_rfc3339_opts(SecondsFormat::Secs, true))
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum HookEvent {
    AfterAgent {
        #[serde(flatten)]
        event: HookEventAfterAgent,
    },
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use chrono::Utc;
    use codex_protocol::ThreadId;
    use codex_utils_absolute_path::test_support::PathBufExt;
    use codex_utils_absolute_path::test_support::test_path_buf;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::HookEvent;
    use super::HookEventAfterAgent;
    use super::HookPayload;

    #[test]
    fn hook_payload_serializes_stable_wire_shape() {
        let session_id = ThreadId::new();
        let thread_id = ThreadId::new();
        let cwd = test_path_buf("/tmp").abs();
        let payload = HookPayload {
            session_id,
            cwd: cwd.clone(),
            client: None,
            triggered_at: Utc
                .with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
                .single()
                .expect("valid timestamp"),
            hook_event: HookEvent::AfterAgent {
                event: HookEventAfterAgent {
                    thread_id,
                    turn_id: "turn-1".to_string(),
                    input_messages: vec!["hello".to_string()],
                    last_assistant_message: Some("hi".to_string()),
                },
            },
        };

        let actual = serde_json::to_value(payload).expect("serialize hook payload");
        let expected = json!({
            "session_id": session_id.to_string(),
            "cwd": cwd.display().to_string(),
            "triggered_at": "2025-01-01T00:00:00Z",
            "hook_event": {
                "event_type": "after_agent",
                "thread_id": thread_id.to_string(),
                "turn_id": "turn-1",
                "input_messages": ["hello"],
                "last_assistant_message": "hi",
            },
        });

        assert_eq!(actual, expected);
    }
}
