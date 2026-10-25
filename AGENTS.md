# Plico Remakes

- This is a Cargo workspace of reimplemented apps in apps/. Record each original repository, reference revision and license in its README and NOTICE.
- Use pinned Plico Git dependencies declared at the workspace root. Changes needed by an app belong upstream in Plico, not copied framework internals.
- Use ordinary synchronous components, context-first widgets and native `+ |cx| { ... }` children. Format with `cargo plico fmt` from the Plico checkout, passing the app's source and test paths.
- The Go reference is https://github.com/hyoban/peek; treat any reference checkout as read-only. Record borrowed code/assets and their MIT license in NOTICE.
- Git operations in this prototype are read-only. Never stage, commit, push, checkout or discard a user's repository from the UI.
- Git IO and syntax highlighting belong to the resident worker. UI receives immutable snapshots and sends typed commands; stale generations must not overwrite new selection.
- CLI: usage-rs. Errors: Snafu. Tests use temporary Git repositories and semantic commands, not OS coordinates or Python scripts.
- Generated reports and captures stay in ignored reports/. Document limitations and comparison scope honestly.

- Never put developer-specific absolute paths in committed files. Use relative paths, environment variables or documented placeholders.
