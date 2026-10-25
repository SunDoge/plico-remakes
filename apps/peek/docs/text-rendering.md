# Glifo dependency comparison

Peek uses the registry version of Glifo 0.4.0 without a Cargo patch. Plico is
pinned to Git tag `v0.1.1`; Cargo.lock records the resolved Git commit.

## Experiment

Compared the same Peek view at 1280 × 800 logical pixels, at scale 1 and scale 2,
using the same detached repository worktree and selected commit. The only changed
dependency was Glifo: crates.io 0.4.0 versus Plico's copy from tag v0.1.1.
Both versions compiled and produced PNG captures through the production Vello path.

The patch flattens hinted glyph outlines at 0.005 physical pixels in the outline
cache. Its removal changes glyph-edge pixels; it does not change shaping, line
positions or the UI layout in these captures. At normal viewing size the difference
is subtle; enlarged samples show different curve edges. This supports using the
registry version for this app, not a claim that all text is pixel-equivalent or
that Plico should drop the patch globally. The initial Peek view contained Latin UI and code text; the additional Chinese
sheet below extends the comparison but does not cover all fonts/platforms.

## Reproduce

Keep the viewed repository, its history and the selected commit unchanged between
captures. From the workspace root, first capture the registry version:

```sh
cargo run --locked -p peek -- /path/to/fixed/repository --commit COMMIT \
  --capture reports/glifo/upstream.png
```

Temporarily select the patched dependency through Cargo configuration:

```sh
cargo --config 'patch.crates-io.glifo.git="https://github.com/SunDoge/plico.git"' \
  --config 'patch.crates-io.glifo.tag="v0.1.1"' \
  run -p peek -- /path/to/fixed/repository --commit COMMIT \
  --capture reports/glifo/patched.png
```

Repeat with `--scale 2`. This temporary patch changes Cargo.lock; save the lockfile
before the experiment and restore that saved copy afterward. Screenshots stay under
ignored reports/ and are not required to read this document.

## Chinese and mixed-language sheet

The `text_rendering` example renders the same Chinese/Latin content without Git,
network requests or app state. It uses Noto Sans CJK SC, sizes 12/14/16/24 and
weights 400/500/700, plus monospace Chinese comments. Tested both light and dark
at scale 1 and 2. Ensure that font is installed; system fallback depends on the host.

```sh
cargo run --locked -p peek --example text_rendering -- \
  --output reports/glifo/chinese-upstream-1x.png
cargo --config 'patch.crates-io.glifo.git="https://github.com/SunDoge/plico.git"' \
  --config 'patch.crates-io.glifo.tag="v0.1.1"' \
  run -p peek --example text_rendering -- \
  --output reports/glifo/chinese-patched-1x.png
```

Repeat with `--scale 2` and/or `--dark`, saving and restoring Cargo.lock as above.
Both variants rendered Chinese successfully with no missing glyphs in this sheet.
Nearest-neighbor enlarged 14px samples show outline-edge differences, especially
curves, diagonals and dense strokes; neither variant fixes the intrinsic loss of
detail at small sizes. Normal-size differences are modest in this sample. Pixel
inequality is not a quality score, so these captures do not establish a universal
winner or justify removing Plico's patch globally. Registry Glifo remains the
simpler default for this comparison app; the workaround remains optional.
