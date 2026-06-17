fill{ path = {{x=0,y=0},{x=300,y=0},{x=300,y=120},{x=0,y=120}}, color = {r=25,g=15,b=15} }
-- L-shaped concave toggle hotspot, to stress concave hit-testing in the live app
region{ path = {{x=30,y=20},{x=150,y=20},{x=150,y=55},{x=85,y=55},{x=85,y=95},{x=30,y=95}},
        on_press = function() host.toggle_sampling() end }
fill{ path = {{x=30,y=20},{x=150,y=20},{x=150,y=55},{x=85,y=55},{x=85,y=95},{x=30,y=95}},
      color = {r=200,g=140,b=60} }
value_fill{ path = {{x=170,y=20},{x=280,y=20},{x=280,y=100},{x=170,y=100}},
            value = "cpu", color = {r=240,g=120,b=120} }
