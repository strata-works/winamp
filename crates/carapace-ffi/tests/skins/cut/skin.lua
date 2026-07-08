-- shaped, draggable backdrop (rounded corners float over the desktop via the transparent base)
fill{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14}, color = {r=12,g=12,b=12} }
-- whole-backdrop drag region (interactive controls drawn later sit on top and win hit-testing)
region{ path = rounded_rect{x=0, y=0, w=300, h=140, radius=14},
        on_press = function() host.begin_drag() end }
-- minimize / close glyphs, top-right
text{ text = "_", x = 270, y = 4, size = 16, color = {r=200,g=200,b=210} }
region{ path = rect{x=266, y=4, w=14, h=16}, on_press = function() host.minimize() end }
text{ text = "x", x = 286, y = 4, size = 16, color = {r=230,g=140,b=140} }
region{ path = rect{x=282, y=4, w=14, h=16}, on_press = function() host.close() end }
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
-- Wrapped multi-line label using the system fallback font (no bundled font named).
text{ text = "carapace\nminimal skin", size = 12, x = 8, y = 8, max_width = 120,
      color = {r = 230, g = 230, b = 230} }
