# Forge development handoff — 2026-10-11

Stopped to preserve the owner's remaining usage quota. **No usable desktop alpha has been delivered.**

## Tested code

Code commit: `044c06b` (parent `df4b5fa`; original baseline `1010ec3`).

- Projects, tasks, criteria and dependencies remain functional.
- Schema v4 adds guarded lifecycle transitions, dependency-aware ready queue, durable runs/events, cancellation requests and explicit interrupted-run recovery. A worker file lock prevents recovery while a worker is live. No automatic replay.
- Schema v5 adds manual model registry, zero-new-spend routing and on-demand Free Router catalog discovery. 124 disabled candidates imported in an actual smoke check. No inferred quota or quality metrics.
- OpenCode adapter launches a credential-isolated temporary profile, denies all tools, captures bounded text proposals, supports timeout/cancellation, and supervises Windows process lifetime with a Job Object. This is proposal-only, not repository-editing execution.
- 34 automated tests passed. Formatting, cargo check, strict Clippy and release build passed on Windows GNU Rust 1.98.1. Exact commands: `cargo fmt --all -- --check`, `cargo check --all-targets`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked --all-targets`, `cargo build --locked --release`.

## Failed or unverified

- Real OpenCode execution is **not proven**. Installed version 1.17.18: isolated longcat-2.5-preview-free lookup failed; mimo-v2.5-free reached the provider and returned HTTP 401 / model unsupported. The live public Zen model list includes newer models absent from the isolated bundled catalog. An explicit model-definition experiment also failed and was reverted. Do not repeat blind model attempts or claim successful real execution.
- No paid APIs were authorized or used. Existing user credentials were not read or inherited. Subscription/API-key model execution remains blocked by policy.
- Native evaluation source exists locally, untracked, under `experiments/ui-bench/`. Cargo dependency resolution remained unfinished and was stopped. Neither prototype was compiled or measured. No toolkit selected. Do not describe these as working GUI features.
- No editor, terminal integration, Git UI, vault, research memory, browser worker or voice implementation yet.
- Windows job assignment has a pre-assignment descendant race; Job Objects are not a filesystem/network sandbox. Tool access remains denied. Unix real-agent supervision is unavailable.
- cargo-deny/gitleaks unavailable; dependency audit, Linux checks, performance and battery measurements not done.

## Exact next step

1. Confirm clean tracked files and remote main. Preserve the untracked native experiments.
2. Build only the egui evaluation package: `cargo build --manifest-path experiments/ui-bench/Cargo.toml --release --features egui --bin egui-bench -j 2`. Diagnose dependency resolution with bounded logs instead of repeatedly restarting it. Slint was separated into `experiments/ui-bench/slint/Cargo.toml` to keep resolution independent.
3. Build the Slint comparison using that manifest, measure both representative workbenches on the laptop, and document evidence before selecting a toolkit. Then integrate the existing library into the native project/task/run screens; prioritize a user-visible working workflow.
4. Diagnose OpenCode's catalog/API mismatch through its documented configuration/version support. A successful anonymous synthetic task is required before claiming live integration. Any existing account credential grant must be explicitly authorized and narrowly scoped.

Continue coherent stages with verified tests, Conventional Commits and non-force fast-forward pushes to main. The owner has authorized ordinary implementation and commits/pushes; paid charges, destructive operations, credential disclosure and security-sensitive grants still require approval.
