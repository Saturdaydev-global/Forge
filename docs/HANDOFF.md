# Forge development handoff — 2026-10-11 (session 2)

Stopped after delivering the native egui workbench window.

## Tested code

Code commit: this session, parent `9d7bd45`.

- `experiments/ui-bench/Cargo.toml`: removed Linux-only `x11` eframe feature and unused `wgpu` feature (which requires a wgpu backend enabled at compile time). The working feature set is `["glow", "default_fonts"]`, which uses OpenGL via glutin on Windows GNU.
- Build command: `cargo build --manifest-path experiments/ui-bench/Cargo.toml --features egui --bin egui-bench`
- Binary: `experiments/ui-bench/target/debug/egui-bench.exe` (196 MB debug, OpenGL/glow backend)
- Runtime verification: process ran at PID 8388, 98 MB working set, no crash. The window opens at 1100×720 pixels.
- All prior library tests still passing. No tracked library code was changed.

## What the window shows

- **Top panel**: label "Forge / native workbench benchmark" + single-line command input
- **Bottom panel**: "Local / no workers / 1,000 lines" status bar
- **Left side panel** (resizable): "Projects" heading, static "Saturday Forge" + "src / domain.rs" labels, 20 selectable task rows "Task N · ready"
- **Right side panel** (resizable): "Review" heading, monospace diff excerpt
- **Central panel**: scrollable multiline text editor seeded with 1,000 representative Rust function lines

## Exact launch command

```
cargo run --manifest-path experiments/ui-bench/Cargo.toml --features egui --bin egui-bench
```

Or directly:

```
experiments\ui-bench\target\debug\egui-bench.exe
```

## Failed or unverified

All items from the previous session remain. No changes to OpenCode integration, provider authentication, or model routing were made this session.

## Exact next steps

1. Wire the left panel project/task list to live SQLite data via the `forge` library (`forge::db::list_projects`, `list_tasks`). Open an async channel or use blocking reads in `update()`.
2. Add a task-detail central panel: show title, criteria checklist, dependency list, status badge.
3. Add a status-bar worker indicator (idle / running / task ID).
4. Collect per-frame paint time with `ctx.input(|i| i.unstable_dt)` and log it to prove the benchmark goal.
5. Once interactive data flow works, measure frame times against the Slint comparison build before selecting a final toolkit.
6. Then integrate the selected toolkit into the main workspace crate (not just the bench).

---

# Forge development handoff — 2026-10-11 (session 3)

Stopped after landing a working ACP client integration with deterministic fixture tests and a live probe against real OpenCode.

## Tested code

- `src/acp.rs`: full ACP client — `detect_harness` (runs `--version` + `acp --help`, bounded by 10s each) and `exercise` (initialize + session/new + optional prompt turn with mid-turn cancel). Uses the official `agent-client-protocol` SDK (v3.3.0, Apache-2.0), driven with `futures::executor::block_on` and `async_io::Timer` timeouts. Permission posture: deny-all (`RequestPermissionOutcome::Cancelled`). Env policy: `extra_envs` only; for OpenCode pass `OPENCODE_DISABLE_AUTOUPDATE=true` to reuse the user's own config/auth.
- `src/bin/acp_fixture_agent.rs`: deterministic token-free fixture agent. `--version` → `0.1.0`; `acp --help` → help line; otherwise a newline-delimited JSON-RPC loop on stdio. Handles `initialize`, `session/new`, `session/prompt` (emits two `agent_message_chunk` notifications; holds response open if prompt contains "cancel"), `session/cancel`, unknown → `-32601`.
- `src/lib.rs`: added `pub mod acp;` and `Error::Acp(String)`.
- `src/main.rs`: added `forge acp detect` / `forge acp probe` subcommands (`--executable`, `--cwd`, `--timeout`, optional `--prompt`, repeatable `--arg`). Made `--db` optional (ACP commands do not touch the database).
- `tests/acp.rs`: 6 integration tests — fixture detect, fixture probe-only, fixture prompt end_turn, fixture prompt cancel-after-one-update, gated real-OpenCode detect, gated real-OpenCode probe. All pass. All 39 pre-existing tests still pass (45 total).

## Live probe result against real OpenCode

```
opencode detect → opencode  1.17.18  acp=true
opencode probe  → agent=OpenCode version=1.17.18 protocol=1
                  session=ses_... (real session created)
                  auth=opencode-login  kind=agent
```

No prompt was sent; zero tokens spent.

## What was fixed during this session

- `session.read_update()` initially received no `session/update` notifications because a client-level `on_receive_notification` handler was consuming them. Removed the handler; `read_update()` via `build_session().run_until()` is the correct path.
- Fixture agent's `session/prompt` params: ACP sends `prompt` as an array of content blocks (not a string). Added `extract_prompt_text` to read `type: "text"` blocks.
- Non-exhaustive `AuthMethod` and `SessionMessage` matches needed wildcard arms.
- `--db` made optional so `acp detect`/`acp probe` work standalone.

## Failed or unverified

- One short read-only real prompt turn against OpenCode has not been run (requires an authenticated free model; gated behind explicit authorization). The probe path is ready — pass `--prompt` to `forge acp probe` once a free model is confirmed available.
- Model enumeration via ACP `configOptions` / `opencode models` not yet exercised.

## Exact next steps

1. If a free authenticated model is available, run one short read-only prompt: `cargo run --bin forge -- acp probe --executable "C:\Users\Administrator\.bun\bin\opencode.exe" --prompt "list the files in the current directory" --timeout 120`.
2. Surface ACP model-selection (provider/model axes) through `configOptions` or `opencode models`.
3. Persist the `ExerciseReport` (harness name/version, protocol, auth methods) into the Forge DB so the task engine can target a known-good harness.
4. Consider gating `forge acp probe --prompt` behind an explicit `--allow-prompt` flag so accidental token spend is impossible.
