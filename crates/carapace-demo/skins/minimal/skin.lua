fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=140},{x=0,y=140}}, color = {r=12,g=12,b=12} }
region{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=80},{x=30,y=80}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=80},{x=30,y=80}}, color = {r=120,g=120,b=120} }
value_fill{ path = {{x=30,y=100},{x=270,y=100},{x=270,y=108},{x=30,y=108}},
            value = "position", color = {r=0,g=220,b=220} }
