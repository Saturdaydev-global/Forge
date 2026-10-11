//! Agent Client Protocol (ACP) harness integration.
//!
//! Distinguishes three dispatch axes, following Firstmate's model:
//!
//! - **Harness** — the agent CLI process speaking ACP (e.g. `opencode acp`).
//!   Detected by executable, version, and `acp` subcommand support.
//! - **Provider/Model** — model registry ids (`provider/model`). Discovered
//!   through the harness's own session config options; never enumerated here.
//! - **Authentication account** — advertised by the harness over ACP
//!   `initialize` as `authMethods` (ids, kinds and names only). Credentials
//!   are never read, copied or exposed by Forge.
//!
//! The client is the smallest working ACP client on the official
//! `agent-client-protocol` SDK (Apache-2.0), driven from synchronous code
//! with `futures::executor::block_on` and `async_io::Timer` timeouts.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AuthMethod, CancelNotification, ContentBlock, ContentChunk, Implementation, InitializeRequest,
    NewSessionRequest, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionNotification, SessionUpdate, StopReason,
};
use agent_client_protocol::util::MatchDispatch;
use agent_client_protocol::{
    AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo, Dispatch, SessionMessage,
};
use futures::executor::block_on;
use futures::future::{self, Either};

use crate::Error;

/// Result of detecting an installed ACP harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessInfo {
    /// Executable stem, e.g. `opencode`.
    pub name: String,
    /// Version string reported by `<exe> --version`, e.g. `1.17.18`.
    pub version: String,
    /// Whether `<exe> acp --help` advertises an ACP server subcommand.
    pub acp_supported: bool,
}

/// An authentication method advertised by a harness over ACP.
///
/// Carries protocol-level identity only; never credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthMethodSummary {
    /// Protocol method id, e.g. `opencode-auth`.
    pub id: String,
    /// `agent` (handled via `authenticate`) or `terminal` (interactive login).
    pub kind: String,
    /// Human-readable name.
    pub name: String,
}

/// Outcome of exercising a full ACP handshake against a harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExerciseReport {
    /// Agent name from `initialize` (`agentInfo.name`), if provided.
    pub agent_name: Option<String>,
    /// Agent version from `initialize` (`agentInfo.version`), if provided.
    pub agent_version: Option<String>,
    /// Negotiated protocol version.
    pub protocol_version: u16,
    /// Authentication methods advertised by the harness.
    pub auth_methods: Vec<AuthMethodSummary>,
    /// Session id created by `session/new`.
    pub session_id: String,
    /// Ids of session config options (the harness's model-selection surface).
    pub config_option_ids: Vec<String>,
    /// Ordered kinds of `session/update` notifications observed.
    pub updates: Vec<String>,
    /// Concatenated text from `agent_message_chunk` updates.
    pub agent_text: String,
    /// Stop reason of the prompt turn, if a prompt was sent.
    pub stop_reason: Option<String>,
    /// Whether Forge sent `session/cancel` during the turn.
    pub cancel_sent: bool,
}

/// Options for [`exercise`].
#[derive(Debug, Clone)]
pub struct ExerciseOptions {
    /// Absolute working directory for `session/new`.
    pub cwd: PathBuf,
    /// Overall timeout for the whole exercise, including process spawn.
    pub timeout: Duration,
    /// Optional user prompt. `None` performs initialize + `session/new` only
    /// and never spends tokens.
    pub prompt: Option<String>,
    /// Send `session/cancel` after this many `session/update` notifications.
    /// Requires `prompt`. `None` never cancels.
    pub cancel_after_updates: Option<usize>,
    /// Extra environment variables for the harness process.
    pub extra_envs: Vec<(String, String)>,
}

/// Detect harness name, version and ACP support for `executable`.
///
/// Runs `<exe> --version` and `<exe> acp --help`, each bounded by 10 seconds.
pub fn detect_harness(executable: &Path) -> Result<HarnessInfo, Error> {
    let version_out = run_capture(
        Command::new(executable).arg("--version"),
        Duration::from_secs(10),
    )?;
    let version = version_out
        .1
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .find(|token| token.chars().any(|c| c.is_ascii_digit()))
        .unwrap_or("")
        .to_string();

    let acp_out = run_capture(
        Command::new(executable).args(["acp", "--help"]),
        Duration::from_secs(10),
    );
    let acp_supported = match acp_out {
        Ok((status, output)) => {
            let lower = output.to_ascii_lowercase();
            status
                && (lower.contains("agent client protocol")
                    || (lower.contains("acp")
                        && !lower.contains("unrecognized subcommand")
                        && !lower.contains("unknown command")))
        }
        Err(_) => false,
    };

    let name = executable
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| executable.to_string_lossy().into_owned());
    Ok(HarnessInfo {
        name,
        version,
        acp_supported,
    })
}

/// Exercise a full ACP connection against a harness: `initialize`,
/// `session/new`, and optionally one prompt turn with mid-turn cancellation.
///
/// `args` are appended to `executable` (e.g. `["acp"]` for OpenCode; empty
/// for an agent binary that speaks ACP on stdio directly).
pub fn exercise(
    executable: &Path,
    args: &[&str],
    options: ExerciseOptions,
) -> Result<ExerciseReport, Error> {
    let timeout = options.timeout;
    let agent = AcpAgent::new(
        AcpAgentConfig::new(executable)
            .args(args.iter().map(|s| s.to_string()))
            .envs(options.extra_envs.clone()),
    );

    // Updates seen via `session.read_update` during the prompt path.
    let prompt_updates: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let prompt_updates_for_main = prompt_updates.clone();

    let prompt = options.prompt.clone();
    let cancel_after = options.cancel_after_updates;
    let cwd = options.cwd.clone();

    let connect = Client
        .builder()
        .name("forge-acp-client")
        .on_receive_request(
            async move |_request: RequestPermissionRequest, responder, _connection| {
                // Deny every permission request: probing harnesses must not
                // earn tool or filesystem access through this client.
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<Agent>| {
            let init = cx
                .send_request(
                    InitializeRequest::new(ProtocolVersion::V1)
                        .client_info(Implementation::new("forge", env!("CARGO_PKG_VERSION"))),
                )
                .block_task()
                .await
                .map_err(map_sdk_error)?;

            let agent_info = init.agent_info;
            let agent_name = agent_info.as_ref().map(|info| info.name.clone());
            let agent_version = agent_info.as_ref().map(|info| info.version.clone());
            let protocol_version = init.protocol_version.as_u16();
            let auth_methods = init
                .auth_methods
                .iter()
                .map(|method| AuthMethodSummary {
                    id: method.id().to_string(),
                    kind: match method {
                        AuthMethod::Agent(_) => "agent".to_string(),
                        AuthMethod::Terminal(_) => "terminal".to_string(),
                        _ => "other".to_string(),
                    },
                    name: method.name().to_string(),
                })
                .collect::<Vec<_>>();

            let turn = if let Some(prompt_text) = prompt.clone() {
                let prompt_updates_for_main = prompt_updates_for_main.clone();
                Some(
                    cx.build_session(&cwd)
                        .block_task()
                        .run_until(async move |mut session| {
                            let session_id = session.session_id().to_string();
                            let config_option_ids = session
                                .config_options()
                                .map(|options| {
                                    options
                                        .iter()
                                        .map(|option| option.id.to_string())
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();

                            session.send_prompt(prompt_text).map_err(map_sdk_error)?;

                            let mut agent_text = String::new();
                            let mut cancel_sent = false;
                            let mut update_count = 0usize;
                            let stop_reason = loop {
                                match session.read_update().await.map_err(map_sdk_error)? {
                                    SessionMessage::SessionMessage(dispatch) => {
                                        let (kind, text) = classify_dispatch(dispatch).await?;
                                        if let Ok(mut updates) = prompt_updates_for_main.lock() {
                                            updates.push(kind);
                                        }
                                        update_count += 1;
                                        if let Some(chunk) = text {
                                            agent_text.push_str(&chunk);
                                        }
                                        if !cancel_sent
                                            && cancel_after.is_some_and(|n| update_count >= n)
                                        {
                                            session
                                                .connection()
                                                .send_notification(CancelNotification::new(
                                                    session.session_id().clone(),
                                                ))
                                                .map_err(map_sdk_error)?;
                                            cancel_sent = true;
                                        }
                                    }
                                    SessionMessage::StopReason(reason) => {
                                        break stop_reason_str(reason).to_string();
                                    }
                                    // SessionMessage is non_exhaustive; ignore
                                    // future variants.
                                    _ => {}
                                }
                            };

                            Ok((
                                session_id,
                                config_option_ids,
                                agent_text,
                                stop_reason,
                                cancel_sent,
                            ))
                        })
                        .await
                        .map_err(map_sdk_error)?,
                )
            } else {
                let response = cx
                    .send_request(NewSessionRequest::new(&cwd))
                    .block_task()
                    .await
                    .map_err(map_sdk_error)?;
                Some((
                    response.session_id.to_string(),
                    response
                        .config_options
                        .as_ref()
                        .map(|options| {
                            options
                                .iter()
                                .map(|option| option.id.to_string())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default(),
                    String::new(),
                    String::new(),
                    false,
                ))
            };

            let (session_id, config_option_ids, agent_text, stop_reason, cancel_sent) =
                turn.expect("prompt and probe paths both produce a turn result");

            let updates = prompt_updates_for_main
                .lock()
                .map(|updates| updates.clone())
                .unwrap_or_default();

            Ok(ExerciseReport {
                agent_name,
                agent_version,
                protocol_version,
                auth_methods,
                session_id,
                config_option_ids,
                updates,
                agent_text,
                stop_reason: (!stop_reason.is_empty()).then_some(stop_reason),
                cancel_sent,
            })
        });

    let outcome = block_on(async {
        match future::select(Box::pin(connect), Box::pin(async_io::Timer::after(timeout))).await {
            Either::Left((result, _timer)) => result,
            Either::Right((_elapsed, connect)) => {
                // Dropping the connect future tears down the AcpAgent
                // transport, which kills the harness child process.
                drop(connect);
                Err(agent_client_protocol::Error::internal_error().data(
                    serde_json::json!({ "reason": "timeout", "timeout_ms": timeout.as_millis() }),
                ))
            }
        }
    });

    outcome.map_err(|error| {
        let message = truncate_chars(&error.to_string(), 2000);
        Error::Acp(message)
    })
}

/// Classify a `Dispatch` from `ActiveSession::read_update` into an update kind
/// and optional agent-message text.
async fn classify_dispatch(
    dispatch: Dispatch,
) -> Result<(String, Option<String>), agent_client_protocol::Error> {
    let mut kind = String::from("other");
    let mut text = None;
    MatchDispatch::new(dispatch)
        .if_notification(async |notification: SessionNotification| {
            kind = update_kind(&notification.update).to_string();
            if let SessionUpdate::AgentMessageChunk(ContentChunk {
                content: ContentBlock::Text(content),
                ..
            }) = &notification.update
            {
                text = Some(content.text.clone());
            }
            Ok(())
        })
        .await
        .otherwise_ignore()?;
    Ok((kind, text))
}

/// Short kind name for a `session/update` variant.
fn update_kind(update: &SessionUpdate) -> &'static str {
    match update {
        SessionUpdate::AgentMessageChunk(_) => "agent_message_chunk",
        SessionUpdate::AgentThoughtChunk(_) => "agent_thought_chunk",
        SessionUpdate::ToolCall(_) => "tool_call",
        SessionUpdate::ToolCallUpdate(_) => "tool_call_update",
        SessionUpdate::Plan(_) => "plan",
        SessionUpdate::AvailableCommandsUpdate(_) => "available_commands_update",
        SessionUpdate::CurrentModeUpdate(_) => "current_mode_update",
        SessionUpdate::ConfigOptionUpdate(_) => "config_option_update",
        SessionUpdate::SessionInfoUpdate(_) => "session_info_update",
        SessionUpdate::UsageUpdate(_) => "usage_update",
        SessionUpdate::Notice(_) => "notice",
        SessionUpdate::CompactionUpdate(_) => "compaction_update",
        SessionUpdate::CompactionSummaryChunk(_) => "compaction_summary_chunk",
        SessionUpdate::UserMessageChunk(_) => "user_message_chunk",
        _ => "other",
    }
}

/// Wire-name of a stop reason.
fn stop_reason_str(reason: StopReason) -> &'static str {
    match reason {
        StopReason::EndTurn => "end_turn",
        StopReason::Cancelled => "cancelled",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        StopReason::Refusal => "refusal",
        _ => "other",
    }
}

fn map_sdk_error(error: agent_client_protocol::Error) -> agent_client_protocol::Error {
    error
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// Run `command` with piped stdio, bounded by `timeout`; return
/// `(success, combined stdout+stderr)`.
fn run_capture(command: &mut Command, timeout: Duration) -> Result<(bool, String), Error> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status.success();
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Acp(format!(
                "harness command timed out after {timeout:?}"
            )));
        }
        std::thread::sleep(Duration::from_millis(25));
    };

    let mut output = String::new();
    use std::io::Read as _;
    drop(stdout.read_to_string(&mut output));
    let mut err_text = String::new();
    drop(stderr.read_to_string(&mut err_text));
    output.push_str(&err_text);
    Ok((status, output))
}
