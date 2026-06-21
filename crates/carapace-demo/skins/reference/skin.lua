-- The genuine Headspace WMP artwork as the faceplate (native 342x394).
image{ asset = "headspace.png", x = 0, y = 0 }

-- Y2K glass sheen across the header band (translucent white, fading down).
fill{ path = {{x=0,y=0},{x=342,y=0},{x=342,y=46},{x=0,y=46}}, gradient = {
  type = "linear", from = {x=0,y=0}, to = {x=0,y=46},
  stops = { {at=0, color={r=255,g=255,b=255, a=110}},
            {at=1, color={r=255,g=255,b=255, a=0}} } } }
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
-- live seek bar bound to position, over the bitmap's seek groove
value_fill{ path = {{x=78,y=216},{x=264,y=216},{x=264,y=230},{x=78,y=230}},
            value = "position", color = {r=120,g=230,b=80} }
-- Gradient-chrome title label (static), centered on the header.
text{ text = "HEADSPACE", font = "vt323.ttf", size = 22, x = 171, y = 6, halign = "center",
      gradient = { type = "linear", from = {x=0,y=0}, to = {x=0,y=22},
                   stops = { {at=0, color={r=235,g=245,b=255}},
                             {at=1, color={r=120,g=150,b=210}} } } }
-- Live value-bound readout: the current track title from host state, left-aligned over the display.
text{ value = "track_title", font = "vt323.ttf", size = 16, x = 78, y = 196,
      color = {r = 120, g = 230, b = 80} }
