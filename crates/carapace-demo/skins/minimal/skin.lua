fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=140},{x=0,y=140}}, color = {r=12,g=12,b=12} }
region{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=80},{x=30,y=80}},
        on_press = function() host.toggle_play() end }
fill{ path = {{x=30,y=30},{x=270,y=30},{x=270,y=80},{x=30,y=80}}, color = {r=120,g=120,b=120} }
value_fill{ path = {{x=30,y=100},{x=270,y=100},{x=270,y=108},{x=30,y=108}},
            value = "position", color = {r=0,g=220,b=220} }
-- A sweep-gradient swatch in the top-right corner (shows the third gradient kind).
fill{ path = {{x=270,y=8},{x=294,y=8},{x=294,y=32},{x=270,y=32}}, gradient = {
  type = "sweep", center = {x=282,y=20}, start_deg = 0, end_deg = 360,
  stops = { {at=0, color={r=255,g=90,b=90}}, {at=0.5, color={r=90,g=130,b=255}},
            {at=1, color={r=255,g=90,b=90}} } } }
