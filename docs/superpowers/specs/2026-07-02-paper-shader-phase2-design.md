# Paper.design Mesh-Gradient — Phase 2 (macOS host) Design

**Date:** 2026-07-02
**Phase 1 spec:** `2026-07-02-paper-shader-transpile-spike-design.md`
**Phase 1 findings (verdict GO):** `2026-07-02-paper-shader-transpile-spike-findings.md`
**Branch:** `worktree-paper-shader-transpile-spike`
**Prior art it rides on:** host-embedding spike #21 (`2026-06-25-host-embedding-spike-findings.md`),
Flutter iOS embed spike (`2026-07-01-flutter-ios-embed-spike-findings.md`)

## The one question Phase 2 answers

Can Phase 1's transpiled paper mesh-gradient render **live and animated inside a real macOS
window**, as the **surround** framing genuine native macOS content — reusing the proven
host-embedding machinery, with **zero diff to the `carapace` engine crate**?

Phase 1 proved the transpile+offscreen-render link (GLSL `300 es` → SPIR-V → WGSL → wgpu,
animated PNG sequence). Phase 2 puts that WGSL on screen in the macOS host.

## The product (what it looks like)

One **borderless macOS window**. Paper's mesh gradient is the **living surround** — a thin
animated frame filling the window edge-to-edge behind everything. A **real, running macOS
media player** (album art, title, scrubber, transport — with actual audio playback) fills
nearly the whole window, wrapped by that gradient border. No window chrome, no app title bar;
the gradient *is* the frame.

Mockup (motion is a canvas stand-in for the real shader):
`docs/superpowers/specs/assets/2026-07-02-paper-shader-phase2-preview.png` *(reference only)*.

## Scope

**A feasibility spike.** Deletable, self-contained, **zero `carapace`-engine diff**. All new
code lives in `crates/embed-spike` (+ its `macos-sample`), which is already the established
spike surface (host-embedding, widget, and Flutter spikes all modified it; the engine crate
stayed untouched). If any task appears to need a `carapace`-crate change → **STOP and report**;
that is a scope violation.

**Explicitly out of scope** (notes-only, not built): the `shader{}` Lua primitive; runtime
glslang / general GLSL→WGSL at load time; sandbox/trust hardening; a true transparent
punch-through `NSView` (vs. composited content surface); MSL/Metal-native render path
(Option B); Windows/Linux; perf tuning beyond a measured observation.

## Decisions locked (from brainstorming)

1. **Render path = Option A (WGSL in wgpu).** Reuse Phase 1 verbatim: the transpiled WGSL runs
   through a wgpu pass in the embed-spike cdylib, which already owns a wgpu device. No new
   transpile target, no Swift/Metal shader reimplementation.
2. **WGSL is pre-baked, not transpiled at runtime.** Commit the exact `@group`-shifted WGSL
   Phase 1's harness produces (`vertex.wgsl` + `mesh_gradient.wgsl`) as vendored assets. This
   matches the Phase 1 findings' production recommendation (glslang is not pure-Rust; don't
   ship it in the runtime) and keeps the spike deterministic. The Phase 1 example remains the
   tool that regenerates them.
3. **Delivery = direct wgpu texture for the shader.** The paper renderer and the engine share
   one wgpu device in one process, so the shader frame is handed to the compositor as a
   `&wgpu::TextureView` (which the engine's `view{}` path already accepts) — no IOSurface
   round-trip for the shader. (The IOSurface handoff is already proven by spike #21;
   re-proving it adds risk, not signal.)
4. **Framed content = a real running macOS player with audio.** A genuine AppKit-rendered
   now-playing view backed by real `AVAudioPlayer` playback + scrub, delivered through the
   proven host-content `view{}` path — the fuller, more convincing demo.

## Architecture

Two composited layers, both carried by the engine's **existing** multi-`view{}` compositor
(`carapace/src/render.rs:554-565` loops every `Node::View` and composites each by id, in scene
order — later nodes draw on top; **verified present, no engine change**):

```
  window surface (IOSurface, Tier-2)
  └─ carapace skin, composited by the engine:
       view{ id="paper"   } — full-bleed  → fed a wgpu TEXTURE of paper's shader  (BEHIND)
       view{ id="content" } — inset ~89%   → fed the host CONTENT IOSurface texture (FRONT)
     (the skin's own vector layer draws ~nothing: maybe a hairline/shadow around the cutout)
```

- **Gradient surround** = `view{ id="paper" }` sized to the whole canvas. embed-spike renders
  paper's pre-baked WGSL into an offscreen `wgpu::Texture` each tick (module `paper_view.rs`),
  advancing `u_time` by wall-clock `dt`, and supplies that texture view for id `"paper"`.
- **macOS content** = `view{ id="content" }` inset to leave the thin gradient border. The Swift
  host renders a real AppKit player into the content IOSurface each tick (as spike #21's host
  draws its clock), reflecting real `AVAudioPlayer` state. The engine imports it as a sampled
  texture (`make_content_texture` + `upload_iosurface_to_texture`, already in `lib.rs`) and
  composites it for id `"content"`.

### Per-frame flow (all in-process, one wgpu device)

```
  carapace_tick(dt):
    u_time += dt
    paper_view.render(u_time)         → paper wgpu texture           (new)
    Swift drawContent() (its own loop) → content IOSurface            (exists, restyled)
    upload content IOSurface → content texture                        (exists)
    render_frame(view_tex = { "paper" → paper_tex, "content" → content_tex })
      → engine draws skin + composites both view{} cutouts            (exists; N-cutout)
      → blit to window IOSurface (Tier 2)                             (exists)
```

## Components

**New (in `crates/embed-spike`):**
- `src/paper_view.rs` — a self-contained module: builds a wgpu render pipeline from the two
  pre-baked WGSL strings against the engine's `wgpu::Device`; owns the uniform buffer(s),
  filled by name at naga offsets with Phase 1's palette/sizing defaults; a 4-corner clip-space
  quad vertex buffer; `render(&self, time: f32) -> &wgpu::TextureView` drawing one frame into
  an owned offscreen texture. Mirrors Phase 1's `harness.rs` pipeline, minus PNG readback.
- Vendored `shaders/mesh_gradient.wgsl` + `shaders/vertex.wgsl` — the committed transpiled,
  `@group`-shifted output (produced by the Phase 1 example; regeneration documented).
- New skin under `crates/embed-spike/skin-*` — declares the two `view{}` nodes (full-bleed
  `paper`, inset `content`) and minimal vector chrome (optional hairline/shadow around the
  cutout). Modeled on the existing `view{}`-cutout skin.

**Modified (in `crates/embed-spike`, spike crate — allowed):**
- `src/render.rs` / `src/lib.rs` — extend the render wrapper from a single hardcoded
  `host_view` to a small map/slice resolving **multiple** view ids (`"paper"` → internal
  paper texture, `"content"` → content IOSurface texture) into one `view_tex` closure. The
  engine already supports N cutouts; this is purely embed-spike plumbing.
- `macos-sample/Sources/EmbedSpike/main.swift` — restyle `drawContent()` from the clock to a
  real player UI (album art, title, scrubber, transport); add `AVAudioPlayer` for real
  playback + scrub; route clicks in the content region to transport controls. Point
  `carapace_create` at the new skin.

**Untouched:** `crates/carapace` (engine) — git-verified zero diff at the end.

## Data flow & animation

`carapace_tick(dt)` already runs on wall-clock `dt` (per the perf-priority stance). The paper
renderer accumulates `u_time += dt` and renders its frame; the engine composites it. Animation
is genuinely time-driven, not frame-counted. Render cadence follows the host loop (the sample
renders continuously; the shader is the animated element that justifies it).

## Gates / evidence

- **P1 — builds & accepts:** cdylib + Swift host build; the paper pipeline is accepted by the
  engine's `wgpu::Device` under a validation error scope; both `view{}` ids resolve.
- **P2 — live in-window, framed:** the macOS window shows the paper mesh gradient as a thin
  living surround around the real player — screenshot evidence.
- **P3 — animated + real audio:** a short screen recording shows the gradient flowing on
  wall-clock time while the player actually plays/scrubs audio; existing embed-spike tests
  still pass; `cargo clippy -p embed-spike -- -D warnings` clean.
- **P4 — perf note:** record the measured cost of driving a full-window shader every frame
  (the surround is the whole window, not a small cutout) — relevant to the perf-priority
  stance; a finding, not a pass/fail bar.

## Risks & mitigations

- **Multi-`view{}` ordering / transparency.** The content must draw *over* the paper surround.
  Mitigation: author the skin with `paper` before `content` (scene order = composite order,
  verified); confirm in P1 with a solid-color content stub before wiring the real player.
- **`u_time` scale / visual fidelity.** Phase 1 already fixed the palette/sizing defaults and
  the `@group(1)` fragment shift; reuse those exact values. Eyeball against Phase 1's
  `/tmp/paper-mesh-spike/mesh_t*.png`.
- **Audio plumbing scope creep.** Keep audio to `AVAudioPlayer` play/pause/scrub on a bundled
  sample clip; no library, no streaming. It exists to make the demo real, not to be robust.
- **wgpu device sharing.** `paper_view` must build its pipeline against the *engine's* device
  (not a second device) so the texture is compositable. Construct it inside `carapace_create`
  after `init_gpu`, holding it in the engine struct.

## Self-review

- **Placeholders:** none. `paper_view.render` returns an owned-texture view; defaults and WGSL
  come from Phase 1 (concrete, not TBD).
- **Consistency:** the two-`view{}` model is used identically in Architecture, Components, and
  Data flow. Zero-engine-diff constraint stated in Scope and reaffirmed per modified file.
- **Scope:** single implementation plan's worth; audio/player kept deliberately minimal.
- **Ambiguity:** "real macOS content" resolved to *AppKit-rendered player drawn into the
  content IOSurface* (proven path), explicitly **not** a transparent `NSView` punch-through
  (called out as out-of-scope).
