-- shaped, draggable backdrop
fill{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14}, color = {r=18, g=22, b=30} }
region{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14},
        on_press = function() host.begin_drag() end }
-- minimize / close
text{ text = "_", x = 270, y = 4, size = 16, color = {r=200,g=200,b=210} }
region{ path = rect{x=266, y=4, w=14, h=16}, on_press = function() host.minimize() end }
text{ text = "x", x = 286, y = 4, size = 16, color = {r=230,g=140,b=140} }
region{ path = rect{x=282, y=4, w=14, h=16}, on_press = function() host.close() end }
-- live metrics, each a one-line gauge extension
gauge{ x = 20,  y = 24, value = "cpu",  label = "CPU" }
gauge{ x = 90,  y = 24, value = "mem",  label = "MEM" }
gauge{ x = 160, y = 24, value = "swap", label = "SWP" }
