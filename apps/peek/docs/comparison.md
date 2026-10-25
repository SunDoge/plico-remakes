# Peek: Go / MyGo and Rust / Plico

Reference: [hyoban/peek](https://github.com/hyoban/peek) `aa19f58c5146e5f5da1047a1c9a2c1fe92309c53`, using
hyoban/mygo `317a82168a5b`. The Rust app uses a pinned Plico Git dependency, starting
from its 0.1.0 public APIs plus the font-family fixes described below.

## What was actually compared

Both apps were captured on Linux, showing the same reference repository and
HEAD commit. Logical viewport is 1280x800, output scale is 2x. The Go side uses
its existing TestSnapshot; the Rust side uses `--capture`. Commands are in the
README. The Go test draws its own title-bar representation; these captures do
not compare native AppKit windows. Rust uses Noto Sans plus a generic monospace
code family. Code text matches Peek’s 12px size and 18px row height.

| Concern | Peek / MyGo | Peek / Plico |
| --- | --- | --- |
| UI | Context-first builders and Children / OnClick callbacks | Context-first builders, `+` children, synchronous `.clicked()` |
| Identity | Explicit contextual keys throughout the app | Component/call-site keys, business keys in dynamic lists |
| View state | Ordinary app and helper structs | Ordinary Application; local handles for path/filter buffers |
| Background work | goroutine returning a UI-thread publication closure through Window.Update | Resident worker receives typed commands and publishes immutable snapshots |
| Stale requests | diffGen, switcher gen, snapshot IDs | Generation-tagged completions; old publications are ignored |
| Code/diff widget | A full pre-existing MyGo editor/DiffView package | Selectable highlighted paragraphs and fixed-row virtual lists; not equivalent editor coverage |
| Native polish | Custom MyGo fork with macOS platform integration | Existing portable Vello window; no AppKit-specific shell work |
| Tests | Original Go UI tester and repository fixtures | Plico Rust semantic driver and temporary Git repositories |

## What the translation demonstrates

The three-column layout translates directly. There was no need to introduce a
new declaration DSL, signal graph or retained application callbacks. Borrowing
an immutable service snapshot while sending commands works through the existing
`snapshot_shared()` API. Children closures are synchronous; the worker owns its
data and never captures UiContext or mutable application state.

State coordination still exists: selected revision/file, viewed flags, folded
folders and source identity need explicit policies in both implementations.
The actor generation discards obsolete results but does not interrupt an already
running Git process. UI controls that request new data are disabled during a load.

Tree rows and visible diff indices are cached against snapshot identity and
filter/viewed state. Only viewport rows enter the declaration tree. None of this
makes the full frame pipeline O(changes).

## Framework change found by a real app

`FontFamily` existed in Plico's shared style, but the Vello adapter always supplied
the global window family. Added Text/control/Layout builders and inheritance,
resolved generic families in the runtime, and fixed the retained paint-cache
comparison to compare against the selected per-node family. A regression test
covers actual adapter output, child inheritance, clean-frame paint reuse and
layout invalidation when the family changes. No new renderer or application
widget trait was needed.

## Performance probe and its limits

`--benchmark` runs 200 warmed, forced snapshot builds of this actual UI. It reports
both total snapshot-build time and Plico's declaration time. Git IO, initial
syntax highlighting, GPU rendering and native event delivery are excluded.
`build_frame` is a snapshot API that intentionally declares the view every call;
this is not an idle-frame benchmark. An idle event-driven native app would not
submit these frames at all.

A local dev-profile (opt-level 1) run was about 107 microseconds per forced
snapshot build, with about 44 microseconds
inside declaration. This is a machine/configuration-specific observation, not a
release build result or a Go-vs-Rust performance claim. Re-run the probe on the
current revision. No matched Go execution benchmark has been performed.

## Remaining gap

The largest gap is editor/DiffView capability, not expressing this layout or
owning state. Both versions now load author avatars in background workers with
disk caching and initials as fallback. The Go editor is substantial framework code already available to
Peek; the current Plico reader does not implement wrapping, expanded context,
split alignment, intraline changes or annotation interaction. The original also
has Codex, native platform and repository lifecycle features absent here. LOC,
launch time and memory totals would be misleading as a winner/loser comparison
until functionality and workloads are matched.
