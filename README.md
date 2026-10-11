# Saturday Forge

Rust CLI foundation for local project and task management. This increment supports project creation and listing plus task creation, lifecycle, dependencies and durable execution records in SQLite.

Requires Rust 1.98.1 and a C toolchain for bundled SQLite. Development currently uses the Windows GNU toolchain with MinGW GCC.

```powershell
cargo build --locked --release
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" project add "Saturday Forge"
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" project list
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" task add --project 1 "Implement routing" --criterion "Routes tasks" --criterion "Rejects paid fallback"
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" task list --project 1
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" task show 1
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" task add --project 1 "Follow-up" --criterion "Done"
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" task dependency add 2 1
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" task dependency list 2
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" task dependency remove 2 1
```

`--db` is required. Its parent directory must exist. A missing database is initialized transactionally at schema version 4; version 1, 2 and 3 databases are migrated transactionally to version 4 without losing projects, tasks or criteria, and newer versions are rejected without migration. Foreign keys are enforced. `task add` inserts the task and all criteria atomically with parameterized SQL. Dependency changes are atomic.

CLI argument errors exit with code 2; validation and database errors exit with code 1. Lock contention waits up to 250 ms before reporting an error. Project and task data is plaintext; do not store secrets. The explicit mock backend simulates execution; it does not execute commands or contact providers. No background service starts automatically.

Project output is one project per line: integer ID, tab, name, ordered by ID. Task `add` prints integer ID, tab, title. Task `list --project <id>` prints one task per line: integer ID, tab, title, tab, `backlog`, ordered by ID. Task `show <id>` prints the task header, numbered criteria in stored order, and a `Dependencies:` section listing direct prerequisite IDs in ascending order (`(none)` when empty). Titles and criteria are trimmed, contain 1–200 Unicode characters, and cannot contain control characters. Every task belongs to an existing project, starts as `backlog`, and requires one or more criteria. A task may depend on another task in the same project; cross-project, self, duplicate and circular dependencies (direct or indirect) are rejected. Unknown projects, unknown tasks, unknown dependencies, invalid titles, invalid criteria and missing criteria return errors without partial inserts.

The library owns name validation and SQLite persistence; the binary handles arguments and output. Migrations use SQLite `user_version` and transactions. The existing GPLv3 LICENSE is preserved; the `only` versus `or-later` designation remains unresolved.

```powershell
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

Tests use temporary databases and separate CLI processes to verify persistence, validation, migration reopening, schema rejection, migration failure, corruption, lock errors, v1/v2 to v3 migration, project preservation, task ordering, project isolation, task failures, dependency validity, isolation, cycles, duplicates, removal, atomicity, foreign keys and database error propagation. Linux compatibility and performance targets have not been measured.

Task execution is explicit: `task status 1 ready`, `task ready --project 1`, `run mock 1`, `run list 1`, `run events 1`, then `task status 1 completed` after review. Mock success is simulated and never proves acceptance criteria. `run cancel <run-id>` requests cancellation; `run recover` requires the worker lock and marks abandoned runs interrupted without replay. Failed/cancelled tasks must explicitly return to ready. Dependencies are immutable during running/review/completed states. One worker owns the database lock; keep the database and lock file in a trusted local directory (do not alias databases with hard links).
