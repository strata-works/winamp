-- The genuine Headspace WMP artwork as the faceplate (native 342x394).
image{ asset = "headspace.png", x = 0, y = 0 }

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
