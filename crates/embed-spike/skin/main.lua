-- Background.
fill{ path = {{x=0,y=0},{x=240,y=0},{x=240,y=80},{x=0,y=80}}, color = {r=18, g=20, b=26} }

-- A horizontal bar whose fill fraction tracks host state key "level" (0.0..1.0).
value_fill{ path = {{x=16,y=16},{x=224,y=16},{x=224,y=40},{x=16,y=40}},
            value = "level", color = {r=120, g=230, b=80} }

-- The whole lower strip is a hotspot that invokes the host action "toggle".
region{ path = {{x=0,y=48},{x=240,y=48},{x=240,y=80},{x=0,y=80}},
        on_press = function() host.toggle() end }
