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

-- ===== Live host view{} cutout INSIDE the artwork's black screen. =====
-- The carapace engine composites the host's OWN live content (rendered by the Swift app into
-- a second IOSurface) into this rect — proving the skin frames real host content.
view{ id = "host", x = 70, y = 60, w = 202, h = 146 }
