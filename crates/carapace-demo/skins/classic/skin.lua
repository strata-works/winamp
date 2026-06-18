fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=140},{x=0,y=140}}, color = {r=24,g=28,b=40} }
region{ path = {{x=20,y=20},{x=90,y=20},{x=90,y=90},{x=20,y=90}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=20,y=20},{x=90,y=20},{x=90,y=90},{x=20,y=90}}, color = {r=80,g=200,b=120} }
region{ path = {{x=110,y=20},{x=180,y=20},{x=180,y=90},{x=110,y=90}},
        on_press = function() host.stop() end }
fill{ path = {{x=110,y=20},{x=180,y=20},{x=180,y=90},{x=110,y=90}}, color = {r=200,g=80,b=80} }
value_fill{ path = {{x=20,y=110},{x=280,y=110},{x=280,y=126},{x=20,y=126}},
            value = "position", color = {r=240,g=220,b=80} }
