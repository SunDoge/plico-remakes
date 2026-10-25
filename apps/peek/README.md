# Peek / Plico

A Rust implementation of [hyoban/peek](https://github.com/hyoban/peek)'s Git browsing
workspace, built with Plico. This is a working comparison project, not a static mockup.

## Run

Run from the `plico-remakes` workspace root. Plico is fetched from Git at the
tag pinned in the root `Cargo.toml`; no sibling checkout is required.

```sh
cargo run -p peek -- /path/to/your/repository --commit HEAD
# Any other local Git repository:
cargo run -p peek -- /path/to/your/repository
```

Requires Git on PATH and the same Linux/native dependencies as Plico's Vello host.
Use Open to enter another repository path, Changes/Files to switch modes, the
history to choose a commit, the file tree to locate code, and F5/Refresh to reload.
The moon/sun button toggles the theme. Both sidebars can be hidden.

## Implemented

- Real Git history, branch and author metadata, full commit messages, changed-file
  statistics, working-tree changes and untracked files.
- Nested/collapsible file tree with filtering, virtualized history and code rows.
- Unified, syntax-highlighted diffs with old/new line numbers and stacked file sections.
- Marking a file Viewed folds its body; selecting it in the tree opens it again.
- Read-only file browsing at a historical commit or in the working tree.
- Selection/copy through Plico's text selection scope, smooth scrolling and light/dark themes.
- Author avatars from GitHub noreply addresses or Gravatar, with a disk cache and
  initials while loading/offline. Avatar requests run independently of Git IO.
- Resident Git worker, immutable Arc snapshots, generation-checked publication.
- Rust semantic interaction tests against temporary real Git repositories.

Git operations are read-only. Ship/Review are visibly disabled; the application
never stages, commits, pushes, checks out or discards changes.

## Comparison and captures

The original application is [hyoban/peek](https://github.com/hyoban/peek). See
[the comparison](docs/comparison.md) for API/state differences and exact scope.

```sh
# Run from the plico-remakes workspace root.
peek_reference=/path/to/peek
remakes_root="$PWD"

# Rust captures: 1280x800 logical pixels, 2x output like the Go snapshot test.
cargo run -p peek -- "$peek_reference" --commit HEAD --scale 2 --capture reports/comparison/plico-light.png
cargo run -p peek -- "$peek_reference" --commit HEAD --dark --scale 2 --capture reports/comparison/plico-dark.png
cargo run -p peek -- "$peek_reference" --commit HEAD --mode files --file internal/app/app.go --scale 2 --capture reports/comparison/plico-files.png

# Original Go app: same repository, HEAD = history row 1.
cd "$peek_reference"
CGO_ENABLED=0 PEEK_REPO=$PWD PEEK_COMMIT=1 \
  PEEK_OUT="$remakes_root/reports/comparison/go" \
  go test ./internal/app -run '^TestSnapshot$' -count=1
```

Captures are local artifacts in ignored `reports/`, not committed fixtures.
The native macOS toolbar, blur and platform text are not reproduced by these
headless captures. Do not infer native-window performance from screenshots.

## Validate

```sh
cargo test
cargo clippy --all-targets -- -D warnings
# From the Plico checkout:
cargo plico fmt --check ../plico-remakes/apps/peek/src ../plico-remakes/apps/peek/tests
# Warm snapshot/declaration probe; excludes Git IO and GPU work:
cargo run -p peek -- /path/to/your/repository --commit HEAD --benchmark
```

For the registry-versus-patched Glifo experiment, see
[the rendering comparison](docs/text-rendering.md).

## Current limits

This first slice is a Git reader, not the full original product. It does not yet
have side-by-side diffs, soft-wrapped code, expandable omitted context, line
comments, inline review findings, image/Markdown previews, native AppKit integration, Codex operations or automatic filesystem
refresh. Long code lines currently ellipsize. Refresh is explicit. Editing is
not implemented. Hot reload has not been wired into this application's host.

History is capped at 100 commits. Stacked diffs are capped at 60 files / 20,000
rows, with an 8 MiB limit on a Git command's output. Git paths are represented as
UTF-8 presentation strings; arbitrary non-UTF-8 Unix filenames are not supported.

Plico uses Git tag `v0.1.1`. The base is its 0.1.0 API; generic
font-family builders/inheritance and Vello's per-node font resolution were added
while implementing the code view. These changes live upstream in Plico, not in private framework copies.

Avatar lookups contact GitHub/Gravatar based on commit email addresses (Gravatar
receives a SHA-256 hash). Successful downloads are cached under the user cache
directory in `peek-plico/avatars-v1`. If Gravatar fails, a unique public-email
match on GitHub is tried before falling back to initials. This fallback sends
the commit email to GitHub’s public user search.
