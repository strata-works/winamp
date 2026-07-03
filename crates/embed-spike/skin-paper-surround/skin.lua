-- Paper mesh-gradient surround (Phase 2). Two engine view{} cutouts:
--   • "paper"   — full-bleed, fed a live wgpu texture of the transpiled paper shader (BEHIND)
--   • "content" — inset, fed the host's real macOS player IOSurface (FRONT)
-- The 24px margin between them is the living gradient border.

-- Full-window drag region (lowest z; controls below win hit-testing).
region{ path = rect{x=0, y=0, w=480, h=300}, on_press = function() host.begin_drag() end }

-- The paper shader fills the whole window, behind everything.
view{ id = "paper", x = 0, y = 0, w = 480, h = 300 }

-- Window controls float over the gradient "titlebar" strip (top 40px) and are drawn as AppKit
-- overlays by the host — carapace vector can't paint over the full-bleed paper shader (the engine
-- composites view{} OVER the vello layer; deferred engine fix). The overlay buttons handle their
-- own clicks, so no skin hotspots here.

-- The real macOS player content, framed by the gradient. Inset 40px at the top to leave the
-- titlebar strip for the floating controls; bottom stays at canvas y=276 (transport unchanged).
view{ id = "content", x = 24, y = 40, w = 432, h = 236 }

-- Transport hotspots over the content (the Swift host draws the glyphs + owns AVAudioPlayer).
-- Rects aligned to the glyphs' ACTUAL canvas positions (Swift draws them at content-local
-- y≈206 + the 24px content offset → canvas y≈230..254; prev/play/next centered at x≈249/281/313).
region{ path = rect{x=234, y=228, w=32, h=32}, on_press = function() host.prev() end }
region{ path = rect{x=266, y=226, w=34, h=36}, on_press = function() host.toggle_play() end }
region{ path = rect{x=300, y=228, w=32, h=32}, on_press = function() host.next() end }
-- Scrub strip over the progress bar (Swift draws the track at content-local y=180 → canvas y≈204).
region{ path = rect{x=188, y=198, w=248, h=22}, on_press = function() host.scrub() end }
-- (Clicking the album art cycles the paper surround shader — handled host-side in mouseUp,
--  since it drives the engine's paper renderer directly, not a skin action. Also the 's' key.)
