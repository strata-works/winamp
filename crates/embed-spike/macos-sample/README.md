# embed-spike macOS sample

Embeds the carapace engine (Rust) in a Swift AppKit app across the C ABI,
using an IOSurface for zero-copy display.

## Build and run

```bash
# 1. Build the Rust dylib (from repo root):
cargo build -p embed-spike

# 2. Run the Swift app:
cd crates/embed-spike/macos-sample
swift run
```

## What you should see

A window (480×160 pt, 240×80 skin pixels) opens showing the spike skin.

- **Green bar** tracks the Mac's battery level (Swift-owned state served
  across the C ABI to the Rust renderer). On a desktop/plugged-in Mac with no
  battery info, it sweeps 0→1 over 10 seconds instead.
- **Click the lower strip** to toggle "paused" — the bar freezes or resumes.
  This exercises the full round-trip: click → Rust engine → Swift `invoke`
  callback → Swift state mutation → Rust reads updated level.
- The console prints:
  - `[carapace] skin dir: <path>` — confirm this resolves to `crates/embed-spike/skin`
  - `[carapace] active tier: 1 (Readback)` or `2 (Shared/Metal)`
  - `[host] invoke: toggle` each time you click the lower strip

## Confirming Tier 1 success

All three of these must hold:

1. Window shows a dark canvas with a green bar (skin rendered by Rust).
2. Bar fill changes over time (Swift state drives Rust pixels).
3. Clicking the lower strip freezes/resumes the bar (Swift invoked through engine).

Capture a screenshot to `crates/embed-spike/screenshot.png` once verified.

## Troubleshooting

- **`carapace_create` returns nil**: the skin path is wrong. Check the printed
  `[carapace] skin dir:` line — it must point to a directory containing
  `main.lua` and `skin.toml`.
- **Dylib not found at launch**: ensure `cargo build -p embed-spike` ran from
  the repo root, and that `target/debug/libembed_spike.dylib` exists.
- **Black window**: `active tier: 0` means engine init failed; see skin path.
