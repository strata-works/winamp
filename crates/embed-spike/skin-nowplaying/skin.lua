-- A compact "now playing" faceplate that renders LIVE host data:
--   text{ value = "key" } pulls a host STRING; value_fill pulls a host NUMBER.
-- Proves a carapace widget can show real information, not just a static bitmap.

-- full-canvas dark backdrop (edge-to-edge; the widget's own rounded mask shapes the corners)
fill{ path = rect{x=0, y=0, w=320, h=140}, color = {r=20, g=22, b=30} }
-- sheen across the whole face
fill{ path = rect{x=0, y=0, w=320, h=140}, gradient = {
  type = "linear", from = {x=0,y=0}, to = {x=0,y=140},
  stops = { {at=0, color={r=70,g=90,b=140, a=70}}, {at=1, color={r=20,g=22,b=30, a=0}} } } }

-- album-art tile
fill{ path = rounded_rect{x=16, y=20, w=100, h=100, radius=10}, color = {r=40, g=46, b=66} }
text{ text = "♪", x = 52, y = 50, size = 40, color = {r=120, g=200, b=255} }

-- header
text{ text = "NOW PLAYING", x = 132, y = 22, size = 12, color = {r=120, g=140, b=180} }
-- bound LIVE text (host string keys)
text{ value = "track",  x = 132, y = 40, size = 22, color = {r=240, g=244, b=255} }
text{ value = "artist", x = 132, y = 70, size = 16, color = {r=150, g=170, b=200} }
-- elapsed / total, right side
text{ value = "time",   x = 132, y = 96, size = 13, color = {r=120, g=140, b=180} }

-- seek bar bound to a host NUMBER (0..1)
fill{ path = rounded_rect{x=132, y=116, w=172, h=8, radius=4}, color = {r=44, g=50, b=70} }
value_fill{ path = rounded_rect{x=132, y=116, w=172, h=8, radius=4}, value = "position",
            color = {r=120, g=200, b=255} }
