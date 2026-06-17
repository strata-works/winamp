-- background
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120},{x=0,y=120}}, color = {r=20,g=30,b=60} }
-- play/pause hotspot (left square)
region{ path = {{x=20,y=20},{x=80,y=20},{x=80,y=80},{x=20,y=80}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=20,y=20},{x=80,y=20},{x=80,y=80},{x=20,y=80}}, color = {r=80,g=200,b=120} }
-- stop hotspot (second square)
region{ path = {{x=100,y=20},{x=160,y=20},{x=160,y=80},{x=100,y=80}},
        on_press = function() host.stop() end }
fill{ path = {{x=100,y=20},{x=160,y=20},{x=160,y=80},{x=100,y=80}}, color = {r=200,g=80,b=80} }
-- position progress bar (bound to host state)
value_fill{ path = {{x=20,y=95},{x=280,y=95},{x=280,y=110},{x=20,y=110}},
            value = "position", color = {r=240,g=240,b=80} }
