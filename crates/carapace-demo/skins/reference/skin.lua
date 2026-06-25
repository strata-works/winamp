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
local SX, SY, SW = 74, 67, 196      -- use the full inner width of the artwork screen
local ACCENT = {r=46, g=212, b=128}   -- the one accent colour (Spotify-ish green)
-- now-playing title (system font, bright) — sized to fit the screen width
text{ value = "track_title", size = 13, x = SX, y = SY, color = {r=242, g=245, b=249} }
-- a slim spectrum band (12 thin bars), accent green, lower-key than before
for i = 0, 11 do
  value_fill{ path = rect{x = SX+2 + i*16, y = SY+20, w = 8, h = 16},
              value = "viz_" .. i, direction = "up", color = ACCENT }
end
-- playlist; the now-playing row is marked by a soft accent highlight bar (no glyph)
list{ collection = "playlist", x = SX, y = SY+44, w = SW, h = 64, row_height = 16,
      on_select = "play_index",
      selected = "current_index", highlight = { r = 46, g = 212, b = 128, a = 44 },
      template = {
        { bind = "title", x = 4, y = 1, size = 12, color = {r=196, g=204, b=214} },
      } }
-- progress: a slim track with an accent-filled played portion
fill{ path = rect{x=SX, y=SY+118, w=SW, h=3}, color = {r=78, g=86, b=98, a=170} }
scrub{ x = SX, y = SY+117, w = SW, h = 4, value = "position", on_seek = "seek", color = ACCENT }
-- elapsed / total time, muted grey
text{ value = "time", size = 11, x = SX+SW, y = SY+124, halign = "right",
      color = {r=140, g=149, b=162} }
