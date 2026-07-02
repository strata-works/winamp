-- Paper mesh-gradient surround (Phase 2). Two engine view{} cutouts:
--   • "paper"   — full-bleed, fed a live wgpu texture of the transpiled paper shader (BEHIND)
--   • "content" — inset, fed the host's real macOS player IOSurface (FRONT)
-- The 24px margin between them is the living gradient border.

-- Full-window drag region (lowest z; controls below win hit-testing).
region{ path = rect{x=0, y=0, w=480, h=300}, on_press = function() host.begin_drag() end }

-- The paper shader fills the whole window, behind everything.
view{ id = "paper", x = 0, y = 0, w = 480, h = 300 }

-- Window controls — invisible hotspots in the top gradient border.
region{ path = rect{x=444, y=6, w=14, h=14}, on_press = function() host.minimize() end }
region{ path = rect{x=462, y=6, w=14, h=14}, on_press = function() host.close() end }

-- The real macOS player content, framed by the gradient.
view{ id = "content", x = 24, y = 24, w = 432, h = 252 }

-- Transport hotspots over the content (the Swift host draws the glyphs + owns AVAudioPlayer).
-- Centered transport row near the content's lower third: prev / play-pause / next.
region{ path = rect{x=212, y=210, w=24, h=24}, on_press = function() host.prev() end }
region{ path = rect{x=240, y=206, w=32, h=32}, on_press = function() host.toggle_play() end }
region{ path = rect{x=276, y=210, w=24, h=24}, on_press = function() host.next() end }
-- Scrub strip over the progress bar.
region{ path = rect{x=180, y=180, w=228, h=14}, on_press = function() host.scrub() end }
