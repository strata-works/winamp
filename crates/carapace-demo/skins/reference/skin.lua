-- The genuine Headspace WMP artwork as the faceplate (native 342x394).
-- The bitmap already provides the chrome: transport glyphs, a black SCREEN cut into the
-- forehead, a seek groove + viz/playlist icons below it, and speakers. We only overlay live
-- content INTO that black screen (no panel of our own — the artwork's screen is the background).
image{ asset = "headspace.png", x = 0, y = 0 }
-- whole-backdrop drag region (interactive controls drawn later sit on top and win hit-testing)
region{ path = rect{x=0, y=0, w=342, h=394}, on_press = function() host.begin_drag() end }
-- minimize / close: invisible hotspots over the faceplate's OWN painted window buttons.
region{ path = rect{x=154, y=4, w=16, h=16}, on_press = function() host.minimize() end }
region{ path = rect{x=172, y=4, w=16, h=16}, on_press = function() host.close() end }

-- Radial glossy sheen over the play transport (Y2K accent).
fill{ path = {{x=148,y=18},{x=184,y=18},{x=184,y=54},{x=148,y=54}}, gradient = {
  type = "radial", center = {x=166,y=36}, radius = 18,
  stops = { {at=0, color={r=255,g=255,b=255, a=150}},
            {at=1, color={r=255,g=255,b=255, a=0}} } } }
-- Transport hotspots traced from the artwork (invisible; the bitmap supplies the glyphs).
region{ path = {{x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48}},
        on_press = function() host.toggle_play() end }
region{ path = {{x=184,y=24},{x=212,y=24},{x=212,y=48},{x=184,y=48}},
        on_press = function() host.stop() end }
region{ path = rect{x=218, y=24, w=24, h=24}, on_press = function() host.prev() end }
region{ path = rect{x=246, y=24, w=24, h=24}, on_press = function() host.next() end }

-- ===== Modern player UI INSIDE the artwork's black screen (x ~68..274, y ~58..205). =====
-- No panel of our own (the bitmap's dark screen is the background); clean system sans-serif,
-- white/grey text with a single green accent, no pixel font or emoji markers.
local SX, SY, SW = 78, 66, 188      -- content area inside the artwork screen
-- now-playing line (VT323 phosphor), sized to fit the screen width
text{ value = "track_title", font = "vt323.ttf", size = 13, x = SX, y = SY,
      color = {r=150, g=250, b=170} }
-- spectrum visualizer: 12 bars filling upward (host viz_<i> levels)
for i = 0, 11 do
  value_fill{ path = rect{x = SX+4 + i*15, y = SY+18, w = 10, h = 24},
              value = "viz_" .. i, direction = "up", color = {r=80, g=240, b=140} }
end
-- separator
fill{ path = rect{x=SX, y=SY+48, w=SW, h=1}, color = {r=70, g=160, b=100, a=170} }
-- playlist; the now-playing row is shown by a phosphor highlight bar (no glyph marker)
list{ collection = "playlist", x = SX, y = SY+52, w = SW, h = 64, row_height = 16,
      on_select = "play_index",
      selected = "current_index", highlight = { r = 36, g = 112, b = 66, a = 175 },
      template = {
        { bind = "title", font = "vt323.ttf", x = 2, y = 1, size = 13, color = {r=190, g=245, b=205} },
      } }
-- foot: thin seek bar + time
fill{ path = rect{x=SX, y=SY+120, w=SW, h=4}, color = {r=0, g=0, b=0, a=140} }
scrub{ x = SX, y = SY+120, w = SW, h = 4, value = "position", on_seek = "seek",
       color = {r=120, g=240, b=130} }
text{ value = "time", font = "vt323.ttf", size = 11, x = SX+SW, y = SY+127, halign = "right",
      color = {r=110, g=210, b=140} }
