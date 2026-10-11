# Development state

Target: integrated native Forge alpha. Authority: project brief; continuous implementation, checks, Conventional Commits and safe fast-forward pushes authorized by owner.

Baseline: 1010ec3 (projects, tasks, criteria, dependencies). Work sequentially on main; preserve LICENSE. No paid calls, credential disclosure, history rewriting or destructive operations.

Stage 1 acceptance: durable guarded lifecycle, dependency-aware ready queue, ordered run journal, cancellation, exclusive worker lock, explicit crash recovery without replay, lossless v3 migration. Mock runs are explicitly simulated and require human acceptance after review.

Next: OpenCode adapter and cost-safe registry; toolkit comparison on this laptop; integrated native editor/workbench; vault and optional integrations after security gates. Native UI and real execution are not yet implemented. Policy is not an OS sandbox.

Every stage: fmt, check, strict clippy, tests, release build; inspect changes and secrets; verify remote main equals local parent before pushing. No autonomous task execution on startup.

Stage 1 committed/pushed: df4b5fa. Lifecycle, ready queue, run journal, mock cancellation and process-crash recovery verified (29 tests at that stage).

Stage 2: model registry and deterministic zero-spend routing; read-only Free Router catalog import; OpenCode adapter with isolated credentials/profile, deny-all tools, bounded proposals, cancellation/timeout and Windows Job Object lifetime supervision. Real calls attempted on 2026-10-11: longcat-2.5-preview-free was missing from the isolated local model catalog; mimo-v2.5-free reached the server but returned HTTP 401 / model unsupported. Neither counts as successful execution. Free Router discovery imported 124 disabled entries. Native benchmarks are being built; no toolkit selected or GUI performance claimed yet.
