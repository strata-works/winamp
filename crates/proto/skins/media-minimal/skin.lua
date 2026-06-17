-- minimalist: dark bg, one wide toggle bar, a thin progress line
fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120},{x=0,y=120}}, color = {r=10,g=10,b=10} }
region{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=70},{x=30,y=70}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=70},{x=30,y=70}}, color = {r=120,g=120,b=120} }
value_fill{ path = {{x=30,y=90},{x=270,y=90},{x=270,y=98},{x=30,y=98}},
            value = "position", color = {r=0,g=220,b=220} }
