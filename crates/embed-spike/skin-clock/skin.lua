-- A clock faceplate driven entirely by LIVE host data:
--   text{ value = "time"/"date" } pull host STRINGS; value_fill pulls the host NUMBER "seconds".
-- Re-rendered with the current time, the output changes every tick — proving live data load.
fill{ path = rect{x=0, y=0, w=320, h=140}, color = {r=14, g=16, b=24} }
fill{ path = rect{x=0, y=0, w=320, h=140}, gradient = {
  type = "linear", from = {x=0,y=0}, to = {x=0,y=140},
  stops = { {at=0, color={r=60,g=80,b=140, a=80}}, {at=1, color={r=14,g=16,b=24, a=0}} } } }

-- big live clock
text{ value = "time", x = 24, y = 30, size = 56, color = {r=235, g=242, b=255} }
-- live date
text{ value = "date", x = 26, y = 96, size = 16, color = {r=140, g=160, b=200} }
-- seconds sweep (0..1 within the current minute)
fill{ path = rounded_rect{x=24, y=120, w=272, h=6, radius=3}, color = {r=40, g=46, b=66} }
value_fill{ path = rounded_rect{x=24, y=120, w=272, h=6, radius=3}, value = "seconds",
            color = {r=120, g=200, b=255} }
