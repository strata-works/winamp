# Total Window Replacement — Spike Findings (2026-06-19)

Spike crate: `crates/window-spike` (throwaway). Platform tested: **macOS / Metal**.
See `crates/window-spike/screenshot.png` for the headline result (the Headspace head
floating as a shaped, chrome-less window with the desktop showing through).

## Headline: total window replacement is FEASIBLE on macOS

A carapace skin renders as a borderless, transparent, **shaped** window — the head floats
with no OS chrome and no bounding rectangle. The only real blocker is **asset authoring**
(the bitmap needs an alpha channel / shape mask), not the windowing/render pipeline.

## Tier 1 — chrome-less transparent window: **WORKED**

- winit attrs: `WindowAttributes::with_decorations(false)` + `with_transparent(true)` — borderless confirmed.
- Surface `alpha_modes` offered by the macOS Metal adapter: **`[Opaque, PostMultiplied]`** — note **`PreMultiplied` is NOT available** on this adapter (wgpu-hal Metal only advertises Opaque + PostMultiplied). The spike's preference order falls back to `PostMultiplied`.
- `alpha_mode` chosen: **`PostMultiplied`**.
- vello `base_color`: transparent `from_rgba8(0, 0, 0, 0)`.
- Pipeline confirmed sound end to end by the **visual result** (desktop shows through): vello → intermediate `Rgba8Unorm` → `wgpu::util::TextureBlitter` → surface. The *likely mechanism* — the blitter using `blend: None` + `write_mask: ColorWrites::ALL` to copy alpha faithfully, and wgpu-hal calling `render_layer.setOpaque(false)` for `PostMultiplied` so the Metal layer honors per-pixel alpha — is **inferred from wgpu 29.0.3 source inspection, not directly observed at runtime**. Treat it as an explanation to re-verify, not a contract, if the wgpu version changes.
- Result: with transparent pixels present, the desktop shows through and the head floats (screenshot). A faint magenta fringe appears on anti-aliased edges — an artifact of the crude runtime color-key (below), not a pipeline fault.

### The real blocker: the asset has no alpha
`crates/carapace-demo/skins/reference/assets/headspace.png` has **no alpha channel**
(`sips hasAlpha: no`) — it is an opaque rectangle with the background baked in. (Classic
WMP skins shaped the window with a *separate region/mask file*, which our asset doesn't
ship.) With the unmodified asset the window was just the head on an opaque rectangle.
To prove the pipeline, the spike applies a **throwaway color-key**: it takes the bitmap's
top-left corner color as the transparent key and zeroes the alpha of near-matching pixels
(`TOL = 28`). That is a spike hack — it can't catch anti-aliased boundary pixels (hence the
fringe) and would punch holes in any interior region that matches the key color.

## render.rs interface gap (for the real phase)
`carapace::render::Renderer::draw` clears to an **opaque black** base
(`RenderParams.base_color = from_rgba8(0,0,0,255)`), which defeats transparency at the
source — the spike had to mirror the pipeline locally to use a transparent base. The real
feature needs the engine renderer to expose a **configurable base color (with alpha)** —
e.g. a `base_color` field on `RenderTarget` or a parameter on `Renderer::draw`, defaulting
to opaque for the normal in-app demo and transparent for window-replacement hosts.

## Tier 2 — drag + window controls: **WORKED (human-confirmed)**
- Code: left-press on the body → `Window::drag_window()`; min-glyph rect → `Window::set_minimized(true)`; close-glyph rect → `event_loop.exit()`. Hit-tested in canvas space (cursor scaled by `bitmap / inner_size`); all standard winit 0.30 calls.
- Runtime (human run): dragging the head body moves the window, and the drawn `_`/`X` glyph rects minimize / quit as expected — confirmed working. The eyeballed canvas-space control rects landed acceptably on the drawn glyphs (they remain trivially tunable for the real phase).

## Tier 3 — click-through verdict: **probe behaved as expected (human-confirmed)**
- `Window::set_cursor_hittest(false)` (toggled with the `t` key) is **whole-window** click-through only — confirmed: toggling it makes the entire window ignore the cursor (clicks pass through everywhere), not the transparent pixels specifically.
- **Per-pixel click-through** (clicks on transparent pixels fall through to apps behind, clicks on the head hit the skin) is **not reachable through winit's cross-platform API**. On macOS it requires native work — e.g. an `NSWindow` with a shaped input region, or feeding the skin's hit-test back into per-event accept/ignore. This is the genuinely hard part and stays a known unknown for the real phase (the engine already owns the shape via hit-testing, which is the raw material for a custom solution).

## Recommendation for the real phase

**Total window replacement is feasible** — pursue it. Required work, in order of certainty:
1. **Engine:** expose a configurable transparent `base_color` on the renderer (small, the one concrete gap this spike found).
2. **Host (winit):** borderless + transparent window with `PostMultiplied` alpha (PreMultiplied unavailable on macOS Metal — code defensively for either); wire window-control host actions (`minimize`/`close`/`begin_drag`) through the engine's `Host`/command boundary rather than the spike's direct winit calls.
3. **Assets/shape:** skins must define their shape — either authored PNGs **with a real alpha channel**, or a **region/color-key mask** pipeline. This is the true prerequisite for the look; the bitmap alone is not enough.
4. **Click-through (optional/hard):** whole-window is free via `set_cursor_hittest`; per-pixel needs platform-specific shaping and should be its own de-risking step if required.

Everything else (vello transparent render, premultiplied/postmultiplied surface compositing, the blitter alpha path) is proven working on macOS/Metal.
