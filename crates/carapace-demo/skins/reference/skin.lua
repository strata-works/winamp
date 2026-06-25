-- The genuine Headspace WMP artwork as the faceplate (native 342x394).
image{ asset = "headspace.png", x = 0, y = 0 }
-- whole-backdrop drag region (interactive controls drawn later sit on top and win hit-testing)
region{ path = rect{x=0, y=0, w=342, h=394}, on_press = function() host.begin_drag() end }
-- minimize / close: invisible hotspots over the faceplate's OWN painted window buttons.
region{ path = rect{x=154, y=4, w=16, h=16}, on_press = function() host.minimize() end }
region{ path = rect{x=172, y=4, w=16, h=16}, on_press = function() host.close() end }

-- Radial glossy highlight over the play transport (Y2K accent).
fill{ path = {{x=148,y=18},{x=184,y=18},{x=184,y=54},{x=148,y=54}}, gradient = {
  type = "radial", center = {x=166,y=36}, radius = 18,
  stops = { {at=0, color={r=255,g=255,b=255, a=170}},
            {at=1, color={r=255,g=255,b=255, a=0}} } } }

-- Transport hotspots traced from the artwork (invisible; the bitmap supplies the glyphs).
region{ path = {{x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48}},
        on_press = function() host.toggle_play() end }
region{ path = {{x=184,y=24},{x=212,y=24},{x=212,y=48},{x=184,y=48}},
        on_press = function() host.stop() end }
region{ path = rect{x=218, y=24, w=24, h=24}, on_press = function() host.prev() end }
region{ path = rect{x=246, y=24, w=24, h=24}, on_press = function() host.next() end }

-- ===== Player HUD, projected directly onto the face (no panel — transparent), centred =====
-- A faint scrim only as wide as the content keeps text legible over the bright dome without
-- reading as a separate window; the face shows through.
local CX, CY, CW = 78, 150, 186     -- content origin (centred lower) + width
fill{ path = rounded_rect{x=CX-6, y=CY-6, w=CW+12, h=150, radius=10}, color = {r=4, g=14, b=8, a=90} }
-- now-playing line (full width), monospace phosphor
text{ value = "track_title", font = "vt323.ttf", size = 12, x = CX, y = CY,
      color = {r=150, g=250, b=170} }
-- spectrum visualizer: 12 bars filling upward (host viz_<i> levels)
for i = 0, 11 do
  value_fill{ path = rect{x = CX+4 + i*15, y = CY+18, w = 10, h = 26},
              value = "viz_" .. i, direction = "up", color = {r=90, g=245, b=150} }
end
-- separator
fill{ path = rect{x=CX, y=CY+50, w=CW, h=1}, color = {r=80, g=180, b=110, a=150} }
-- playlist (the now-playing row gets a highlight bar)
list{ collection = "playlist", x = CX, y = CY+54, w = CW, h = 64, row_height = 16,
      on_select = "play_index",
      selected = "current_index", highlight = { r = 40, g = 120, b = 70, a = 150 },
      template = {
        { bind = "now",   font = "vt323.ttf", x = 0,  y = 1, size = 13, color = {r=130,g=250,b=120} },
        { bind = "title", font = "vt323.ttf", x = 15, y = 1, size = 13, color = {r=205,g=240,b=212} },
      } }
-- foot: thin seek bar + time
scrub{ x = CX, y = CY+124, w = CW, h = 5, value = "position", on_seek = "seek",
       color = {r=120, g=245, b=140} }
text{ value = "time", font = "vt323.ttf", size = 11, x = CX+CW, y = CY+131, halign = "right",
      color = {r=120, g=215, b=145} }
