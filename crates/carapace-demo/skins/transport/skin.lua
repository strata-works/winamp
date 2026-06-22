-- A backdrop plus a host-registered extension: one declaration is a full transport,
-- wired to the host's own actions (toggle_play / stop) with zero Lua glue.
-- shaped, draggable backdrop (rounded corners float over the desktop via the transparent base)
fill{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14}, color = {r=20, g=24, b=34} }
-- whole-backdrop drag region (interactive controls drawn later sit on top and win hit-testing)
region{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14},
        on_press = function() host.begin_drag() end }
-- minimize / close glyphs, top-right
text{ text = "_", x = 270, y = 4, size = 16, color = {r=200,g=200,b=210} }
region{ path = rect{x=266, y=4, w=14, h=16}, on_press = function() host.minimize() end }
text{ text = "x", x = 286, y = 4, size = 16, color = {r=230,g=140,b=140} }
region{ path = rect{x=282, y=4, w=14, h=16}, on_press = function() host.close() end }
transport{ x = 20, y = 20 }
