-- The genuine Headspace WMP artwork as the faceplate (native 342x394).
image { asset = "headspace.png", x = 0, y = 0 }
-- whole-backdrop drag region (interactive controls drawn later sit on top and win hit-testing)
region { path = rect { x = 0, y = 0, w = 342, h = 394 }, on_press = function() host.begin_drag() end }
-- minimize / close: invisible hotspots over the faceplate's OWN painted window buttons
-- (the □ and X at top-center), so the chrome looks native — no drawn glyphs needed.
region { path = rect { x = 154, y = 4, w = 16, h = 16 }, on_press = function() host.minimize() end }
region { path = rect { x = 172, y = 4, w = 16, h = 16 }, on_press = function() host.close() end }

-- Radial glossy highlight over the play transport (Y2K accent).
fill { path = { { x = 148, y = 18 }, { x = 184, y = 18 }, { x = 184, y = 54 }, { x = 148, y = 54 } }, gradient = {
    type = "radial", center = { x = 166, y = 36 }, radius = 18,
    stops = { { at = 0, color = { r = 255, g = 255, b = 255, a = 170 } },
        { at = 1, color = { r = 255, g = 255, b = 255, a = 0 } } } } }

-- Transport hotspots traced from the artwork (invisible; the bitmap supplies the glyphs).
region { path = { { x = 150, y = 24 }, { x = 178, y = 24 }, { x = 178, y = 48 }, { x = 150, y = 48 } },
    on_press = function() host.toggle_play() end }
region { path = { { x = 184, y = 24 }, { x = 212, y = 24 }, { x = 212, y = 48 }, { x = 184, y = 48 } },
    on_press = function() host.stop() end }
region { path = rect { x = 218, y = 24, w = 24, h = 24 }, on_press = function() host.prev() end }
region { path = rect { x = 246, y = 24, w = 24, h = 24 }, on_press = function() host.next() end }

-- ===== The CRT screen: a dark phosphor display set into the faceplate's window. =====
-- Everything the player shows lives here: now-playing line, tracklist, seek bar, time.
-- panel, rounded to follow the head
fill { path = rounded_rect { x = 64, y = 60, w = 214, h = 158, radius = 16 }, color = { r = 6, g = 18, b = 12, a = 237 } }
-- CRT scanlines: thin dark lines every 3px (a Lua loop emitting many fills — the engine
-- handles an unbounded node count; this is pure declarative scenery, no per-frame cost).
for y = 64, 214, 3 do
    fill { path = rect { x = 68, y = y, w = 206, h = 1 }, color = { r = 0, g = 0, b = 0, a = 55 } }
end
-- now-playing line (host state), phosphor green, monospace
text { value = "track_title", font = "vt323.ttf", size = 15, x = 78, y = 66,
    color = { r = 130, g = 245, b = 150 } }
-- separator under the header
fill { path = rect { x = 78, y = 86, w = 184, h = 1 }, color = { r = 60, g = 150, b = 95, a = 170 } }
-- clickable playlist (host-driven rows); clicking a row plays that track
list { collection = "playlist", x = 78, y = 92, w = 184, h = 76, row_height = 18,
    on_select = "play_index",
    selected = "current_index", highlight = { r = 36, g = 110, b = 64, a = 150 },
    template = {
        { bind = "now",   font = "vt323.ttf", x = 0,  y = 1, size = 14, color = { r = 120, g = 245, b = 110 } },
        { bind = "title", font = "vt323.ttf", x = 16, y = 1, size = 14, color = { r = 190, g = 235, b = 200 } },
    } }
-- in-screen seek bar: a dark groove with a click-to-seek phosphor fill
fill { path = rect { x = 78, y = 192, w = 184, h = 6 }, color = { r = 0, g = 0, b = 0, a = 120 } }
scrub { x = 78, y = 192, w = 184, h = 6, value = "position", on_seek = "seek",
    color = { r = 120, g = 240, b = 130 } }
-- elapsed / total time
text { value = "time", font = "vt323.ttf", size = 13, x = 78, y = 200,
    color = { r = 110, g = 210, b = 130 } }

-- Gradient-chrome title label (static), centered on the header.
-- text { text = "HEADSPACE", font = "vt323.ttf", size = 22, x = 171, y = 6, halign = "center",
--     gradient = { type = "linear", from = { x = 0, y = 0 }, to = { x = 0, y = 22 },
--         stops = { { at = 0, color = { r = 235, g = 245, b = 255 } },
--             { at = 1, color = { r = 120, g = 150, b = 210 } } } } }
