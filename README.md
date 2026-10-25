# Plico Remakes

Reimplementations of open-source applications with [Plico](https://github.com/SunDoge/plico),
used to test and improve the framework through real applications.

Each app documents its original repository, reference revision, license, implemented
scope and remaining differences. These are independent implementations, not official
versions of the original applications. Asset licenses are recorded separately.

## Applications

| Member | Original application | License | Scope |
| --- | --- | --- | --- |
| [`peek`](apps/peek) | [hyoban/peek](https://github.com/hyoban/peek) | MIT; see [NOTICE](apps/peek/NOTICE) | Read-only Git history, files and highlighted diffs |

## Run

```sh
cargo run -p peek -- /path/to/your/repository
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Git must be on PATH. Native build dependencies are the same as Plico's Vello host.
Plico uses Git tag `v0.1.1`, pinned in the root `Cargo.toml`; no local
Plico checkout is required. Glifo comes directly from crates.io without a patch. The workspace lockfile is committed.

See [Peek's README](apps/peek/README.md) for capture commands and current limitations,
and its [comparison](apps/peek/docs/comparison.md) for the Go/Rust comparison.
Generated captures and reports are not committed.
