//! Integration tests for the Forge ACP client against the deterministic
//! fixture agent and (gated) the real OpenCode harness.
//!
//! The fixture agent is a Rust bin in this crate (`acp_fixture_agent`) and
//! never spends tokens. The OpenCode tests are skipped when the executable
//! is not present.

use std::path::PathBuf;
use std::time::Duration;

use forge::acp::{ExerciseOptions, detect_harness, exercise};

fn fixture_agent() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_acp_fixture_agent"))
}

fn fixture_options(prompt: Option<&str>) -> ExerciseOptions {
    ExerciseOptions {
        cwd: std::env::current_dir().expect("current dir"),
        timeout: Duration::from_secs(30),
        prompt: prompt.map(str::to_string),
        cancel_after_updates: None,
        extra_envs: Vec::new(),
    }
}

#[test]
fn detect_fixture_harness() {
    let info = detect_harness(&fixture_agent()).expect("detect fixture");
    assert_eq!(info.name, "acp_fixture_agent");
    assert_eq!(info.version, "0.1.0");
    assert!(info.acp_supported, "fixture advertises acp via --help");
}

#[test]
fn probe_only_no_prompt() {
    let report = exercise(&fixture_agent(), &[], fixture_options(None)).expect("probe");
    assert_eq!(report.agent_name.as_deref(), Some("forge-fixture-agent"));
    assert_eq!(report.agent_version.as_deref(), Some("0.1.0"));
    assert_eq!(report.protocol_version, 1);
    assert_eq!(report.session_id, "fixture-session");
    assert_eq!(report.auth_methods.len(), 1);
    assert_eq!(report.auth_methods[0].id, "fixture-none");
    assert_eq!(report.auth_methods[0].kind, "agent");
    assert!(report.stop_reason.is_none(), "no prompt sent");
    assert!(report.updates.is_empty());
}

#[test]
fn prompt_end_turn() {
    let report = exercise(
        &fixture_agent(),
        &[],
        fixture_options(Some("hello from forge")),
    )
    .expect("prompt end_turn");
    assert_eq!(report.stop_reason.as_deref(), Some("end_turn"));
    assert!(!report.cancel_sent);
    assert_eq!(
        report.agent_text,
        "fixture-progress-1fixture-progress-2fixture-reply: hello from forge"
    );
    assert!(report.updates.contains(&"agent_message_chunk".to_string()));
}

#[test]
fn prompt_cancel_after_one_update() {
    let mut options = fixture_options(Some("please cancel me"));
    options.cancel_after_updates = Some(1);
    let report = exercise(&fixture_agent(), &[], options).expect("prompt cancel");
    assert_eq!(report.stop_reason.as_deref(), Some("cancelled"));
    assert!(report.cancel_sent, "cancel was sent mid-turn");
}

// ---------------------------------------------------------------------------
// Real OpenCode harness — skipped when not installed.
// ---------------------------------------------------------------------------

fn opencode_exe() -> Option<PathBuf> {
    let candidate = PathBuf::from(r"C:\Users\Administrator\.bun\bin\opencode.exe");
    candidate.exists().then_some(candidate)
}

#[test]
fn detect_real_opencode() {
    let Some(exe) = opencode_exe() else {
        eprintln!("skipping: opencode not found");
        return;
    };
    let info = detect_harness(&exe).expect("detect opencode");
    assert_eq!(info.name, "opencode");
    assert!(!info.version.is_empty());
    assert!(info.acp_supported, "opencode advertises acp");
}

#[test]
fn probe_real_opencode_initialize_and_session_new() {
    let Some(exe) = opencode_exe() else {
        eprintln!("skipping: opencode not found");
        return;
    };
    let options = ExerciseOptions {
        cwd: std::env::current_dir().expect("current dir"),
        timeout: Duration::from_secs(60),
        prompt: None, // initialize + session/new only — never spends tokens
        cancel_after_updates: None,
        extra_envs: vec![(
            "OPENCODE_DISABLE_AUTOUPDATE".to_string(),
            "true".to_string(),
        )],
    };
    let report = exercise(&exe, &["acp"], options).expect("probe opencode");
    assert_eq!(report.protocol_version, 1);
    assert!(!report.session_id.is_empty());
    // OpenCode may or may not provide agentInfo; authMethods and session id
    // are the real assertions.
}
