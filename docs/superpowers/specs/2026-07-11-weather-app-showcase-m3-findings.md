# M3 Task 1 Spike — Transparency GO/NO-GO Finding

**Date:** 2026-07-11
**Question:** Does the engine's 4-stage composite carry a shader-authored `alpha < 1`
all the way through to a genuinely transparent window region (so the bottom-flowing
silhouette can shape the window via the shader's own alpha, with zero engine changes)?

## Verdict: **GO** ✅

The bottom 15% of the window (temporarily forced to premultiplied `alpha = 0` in the
shader's `fs()`) is genuinely see-through: over a controlled bright-magenta backdrop,
the magenta shows *through* the window in the `uv.y > 0.85` band. Shader alpha shapes
the transparent window. The signature flowing silhouette (Task 4) is feasible with zero
engine-crate changes.

## What was tested

- Temporarily edited `weather/skins/weather/assets/weather.wgsl` `fs()` to return
  `alpha = 0` (premultiplied) for `in.uv.y > 0.85`, and `App.swift` `window.hasShadow = false`.
- Built (`cargo build -p carapace-ffi && swift build`) and launched the app.
- The host is already configured for transparency: `NSWindow.isOpaque = false` +
  `backgroundColor = .clear` (App.swift), the layer-backed view is `isOpaque = false`
  with a clear background (SkinView.swift), and the frame surfaces are alpha-capable
  **BGRA** IOSurfaces (CarapaceBridge.swift). So alpha only had to survive the engine's
  render→composite→surface path — which it does.

## Evidence

- `/tmp/m3-spike-magenta.png` — the definitive shot: solid-magenta backdrop visible
  left and right of the window (confirming it sits *behind* the window); the window's
  bottom band (below the "Fri" row, exactly the `uv.y > 0.85` region) shows the magenta
  backdrop through it. (An unrelated floating media-player window happened to overlap the
  band's right half — the transparent magenta is unambiguous on the left half.)
- Earlier `/tmp/m3-spike.png` and `/tmp/m3-spike-wide.png` read as a "black band" — a
  **false negative**: the desktop behind the window was a dark code editor / dark floating
  windows, so a transparent band showing dark content looked identical to opaque black.
  The controlled bright backdrop resolved the ambiguity.

## Notes for the rest of M3

- **Path correction:** the plan refers to `weather/skins/weather/weather.wgsl`; the actual
  file is `weather/skins/weather/assets/weather.wgsl`. Tasks 4 (rewrite) and the Task 1
  revert use this real path.
- Temp spike edits reverted (`git checkout`); working tree clean. Only this findings doc
  is committed for Task 1.
