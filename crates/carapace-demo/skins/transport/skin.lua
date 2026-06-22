-- A backdrop plus a host-registered extension: one declaration is a full transport,
-- wired to the host's own actions (toggle_play / stop) with zero Lua glue.
fill{ path = rect{x=0, y=0, w=300, h=140}, color = {r=20, g=24, b=34} }
transport{ x = 20, y = 20 }
