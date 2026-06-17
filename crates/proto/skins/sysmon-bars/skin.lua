fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120},{x=0,y=120}}, color = {r=15,g=15,b=25} }
-- click anywhere on the big panel toggles sampling
region{ path = {{x=20,y=20},{x=280,y=20},{x=280,y=60},{x=20,y=60}},
        on_press = function() host.toggle_sampling() end }
fill{ path = {{x=20,y=20},{x=280,y=20},{x=280,y=60},{x=20,y=60}}, color = {r=40,g=60,b=90} }
-- cpu meter
value_fill{ path = {{x=20,y=75},{x=280,y=75},{x=280,y=105},{x=20,y=105}},
            value = "cpu", color = {r=120,g=240,b=120} }
