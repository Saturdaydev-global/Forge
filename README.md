# Saturday Forge

Rust CLI foundation for local project management. This increment supports only project creation and listing in SQLite.

Requires Rust 1.98.1 and a C toolchain for bundled SQLite. Development currently uses the Windows GNU toolchain with MinGW GCC.

```powershell
cargo build --locked --release
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" project add "Saturday Forge"
.\target\release\forge.exe --db "$env:TEMP\forge-demo.sqlite" project list
```

`--db` is required. Its parent directory must exist. A missing database is initialized transactionally at schema version 1; newer versions are rejected without migration. Output is one project per line: integer ID, tab, name, ordered by ID. Names are trimmed, contain 1–200 Unicode characters, and cannot contain control characters. Duplicate names are allowed; IDs identify projects.

CLI argument errors exit with code 2; validation and database errors exit with code 1. Lock contention waits up to 250 ms before reporting an error. Project data is plaintext; do not store secrets. Forge does not execute commands, contact providers, or run background services.

The library owns name validation and SQLite persistence; the binary handles arguments and output. Migrations use SQLite `user_version` and transactions. The existing GPLv3 LICENSE is preserved; the `only` versus `or-later` designation remains unresolved.

```powershell
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

Tests use temporary databases and separate CLI processes to verify persistence, validation, migration reopening, schema rejection, migration failure, corruption and lock errors. Linux compatibility and performance targets have not been measured.
