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
