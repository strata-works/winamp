-- shaped, draggable backdrop (rounded corners float over the desktop via the transparent base)
fill{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14}, color = {r=24, g=28, b=40} }
-- whole-backdrop drag region (interactive controls drawn later sit on top and win hit-testing)
region{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14},
        on_press = function() host.begin_drag() end }
-- minimize / close glyphs, top-right
text{ text = "_", x = 270, y = 4, size = 16, color = {r=200,g=200,b=210} }
region{ path = rect{x=266, y=4, w=14, h=16}, on_press = function() host.minimize() end }
text{ text = "x", x = 286, y = 4, size = 16, color = {r=230,g=140,b=140} }
region{ path = rect{x=282, y=4, w=14, h=16}, on_press = function() host.close() end }

-- play button: one declaration is both drawn and clickable (was region{}+fill{})
fill{ path = rect{x=20, y=20, w=70, h=70}, color = {r=80, g=200, b=120},
      on_press = function() host.toggle_play() end }

-- stop button: a rounded chrome rect, also click-as-draw
fill{ path = rounded_rect{x=110, y=20, w=70, h=70, radius=12}, color = {r=200, g=80, b=80},
      on_press = function() host.stop() end }

-- a circular knob (decorative shape helper)
fill{ path = circle{cx=240, cy=55, r=28}, color = {r=180, g=180, b=70} }

-- horizontal seek bar bound to position
value_fill{ path = rect{x=20, y=110, w=260, h=16}, value = "position",
            color = {r=240, g=220, b=80} }

-- vertical meter bound to position, growing upward
value_fill{ path = rect{x=284, y=20, w=10, h=100}, value = "position", direction = "up",
            color = {r=120, g=230, b=200} }
