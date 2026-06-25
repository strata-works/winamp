-- The genuine Headspace WMP artwork as the faceplate (native 342x394).
image{ asset = "headspace.png", x = 0, y = 0 }
-- whole-backdrop drag region (interactive controls drawn later sit on top and win hit-testing)
region{ path = rect{x=0, y=0, w=342, h=394}, on_press = function() host.begin_drag() end }
-- minimize / close: invisible hotspots over the faceplate's OWN painted window buttons
-- (the □ and X at top-center), so the chrome looks native — no drawn glyphs needed.
region{ path = rect{x=154, y=4, w=16, h=16}, on_press = function() host.minimize() end }
region{ path = rect{x=172, y=4, w=16, h=16}, on_press = function() host.close() end }

-- (The old full-width "glass sheen" linear gradient was removed: with the alpha-cut faceplate
-- its top corners now fall on transparent pixels and showed as a white bar. The radial glossy
-- below sits on the green dome, so it stays as the Y2K accent.)
-- Radial glossy highlight over the play transport.
fill{ path = {{x=148,y=18},{x=184,y=18},{x=184,y=54},{x=148,y=54}}, gradient = {
  type = "radial", center = {x=166,y=36}, radius = 18,
  stops = { {at=0, color={r=255,g=255,b=255, a=170}},
            {at=1, color={r=255,g=255,b=255, a=0}} } } }
-- Invisible interactive overlays on top of the bitmap (positions traced from the artwork):
-- play/pause hotspot over the transport area
region{ path = {{x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48}},
        on_press = function() host.toggle_play() end }
-- stop hotspot
region{ path = {{x=184,y=24},{x=212,y=24},{x=212,y=48},{x=184,y=48}},
        on_press = function() host.stop() end }
-- live, click-to-seek bar bound to position, over the bitmap's seek groove
scrub{ x = 78, y = 216, w = 186, h = 14, value = "position", on_seek = "seek",
       color = {r=120,g=230,b=80} }
-- previous / next track hotspots (positioned over the artwork's transport area; tune to taste)
region{ path = rect{x=218, y=24, w=24, h=24}, on_press = function() host.prev() end }
region{ path = rect{x=246, y=24, w=24, h=24}, on_press = function() host.next() end }
-- elapsed / total time readout
text{ value = "time", font = "vt323.ttf", size = 13, x = 78, y = 232,
      color = {r = 120, g = 230, b = 80} }
-- The player "screen": a translucent green-tinted glass panel over the faceplate's display
-- window (the face shows through faintly — Y2K glass), hosting the now-playing title + the
-- clickable playlist. (The system monitor is no longer composited here; it remains demoable
-- via the H-key standalone sysmon skin.)
fill{ path = rounded_rect{x=70, y=63, w=202, h=142, radius=11},
      color = {r=8, g=26, b=18, a=200} }
-- faint inner top-edge sheen for the glass feel
fill{ path = rect{x=78, y=68, w=186, h=1}, color = {r=150, g=230, b=180, a=70} }
-- now-playing title (host state), as the screen header, in bright phosphor green
text{ value = "track_title", font = "vt323.ttf", size = 14, x = 82, y = 71,
      color = {r=150, g=245, b=175} }
-- separator under the header
fill{ path = rect{x=82, y=90, w=178, h=1}, color = {r=90, g=170, b=120, a=150} }
-- clickable playlist (host-driven rows) inside the screen; clicking a row plays that track
list{ collection = "playlist", x = 82, y = 96, w = 178, h = 104, row_height = 20,
      on_select = "play_index",
      template = {
        { bind = "now",   x = 0,  y = 2, size = 13, color = {r=120,g=235,b=110} },
        { bind = "title", x = 16, y = 2, size = 13, color = {r=205,g=235,b=212} },
      } }
-- Gradient-chrome title label (static), centered on the header.
text{ text = "HEADSPACE", font = "vt323.ttf", size = 22, x = 171, y = 6, halign = "center",
      gradient = { type = "linear", from = {x=0,y=0}, to = {x=0,y=22},
                   stops = { {at=0, color={r=235,g=245,b=255}},
                             {at=1, color={r=120,g=150,b=210}} } } }
