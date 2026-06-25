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

-- ===== The CRT screen (compact, set into the faceplate's forehead window). =====
-- Top: now-playing line + a spectrum visualizer. Middle: the playlist. Foot: seek bar + time.
-- Sized to clear the speakers, the transport, and the artwork's seek groove.
local SX, SY, SW, SH = 86, 62, 170, 140
fill{ path = rounded_rect{x=SX, y=SY, w=SW, h=SH, radius=13}, color = {r=6, g=18, b=12, a=235} }
-- CRT scanlines
for y = SY + 4, SY + SH - 6, 3 do
  fill{ path = rect{x=SX+4, y=y, w=SW-8, h=1}, color = {r=0, g=0, b=0, a=55} }
end
-- now-playing line (full width), monospace phosphor
text{ value = "track_title", font = "vt323.ttf", size = 11, x = SX+8, y = SY+3,
      color = {r=130, g=245, b=150} }
-- spectrum visualizer: 12 bars filling upward (host viz_<i> levels)
for i = 0, 11 do
  value_fill{ path = rect{x = SX+10 + i*12, y = SY+20, w = 8, h = 22},
              value = "viz_" .. i, direction = "up", color = {r=70, g=235, b=140} }
end
-- separator
fill{ path = rect{x=SX+8, y=SY+46, w=SW-16, h=1}, color = {r=60, g=150, b=95, a=160} }
-- playlist (the now-playing row gets a highlight bar)
list{ collection = "playlist", x = SX+8, y = SY+50, w = SW-16, h = 60, row_height = 15,
      on_select = "play_index",
      selected = "current_index", highlight = { r = 34, g = 104, b = 60, a = 150 },
      template = {
        { bind = "now",   font = "vt323.ttf", x = 0,  y = 1, size = 12, color = {r=120,g=245,b=110} },
        { bind = "title", font = "vt323.ttf", x = 13, y = 1, size = 12, color = {r=195,g=232,b=205} },
      } }
-- foot: thin seek bar + time
fill{ path = rect{x=SX+8, y=SY+116, w=SW-16, h=4}, color = {r=0, g=0, b=0, a=130} }
scrub{ x = SX+8, y = SY+116, w = SW-16, h = 4, value = "position", on_seek = "seek",
       color = {r=120, g=240, b=130} }
text{ value = "time", font = "vt323.ttf", size = 11, x = SX+SW-8, y = SY+123, halign = "right",
      color = {r=95, g=195, b=125} }
