fill{ path = rounded_rect{x=0, y=0, w=420, h=660, radius=18}, color = {r=30, g=20, b=28} }
region{ path = rounded_rect{x=0, y=0, w=420, h=660, radius=18}, role='drag',
        on_press = function() host.begin_drag() end }
text{ text="_", x=384, y=8, size=16, color={r=210,g=195,b=205} }
region{ path=rect{x=380,y=8,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=402, y=8, size=16, color={r=235,g=150,b=150} }
region{ path=rect{x=398,y=8,w=16,h=18}, on_press=function() host.close() end }
text{ value="track_title", x=24, y=48, size=26, color={r=255,g=225,b=170} }
text{ value="artist", x=24, y=84, size=16, color={r=200,g=150,b=175} }
text{ value="time", x=24, y=112, size=13, color={r=150,g=120,b=140} }
scrub{ value="position", on_seek="seek", x=24, y=140, w=372, h=12,
       direction='right', color={r=255,g=170,b=120} }
fill{ path=rect{x=24, y=170, w=372, h=52}, color={r=255,g=170,b=120},
      on_press=function() host.toggle_play() end }
text{ text="play / pause", x=150, y=186, size=15, color={r=40,g=20,b=10} }
list{ collection="playlist", x=24, y=240, w=372, h=396, row_height=36,
      on_select="play_index", highlight={r=64,g=40,b=52}, selected="current_index",
      template={
        { bind='now', x=8, y=9, size=16, color={r=255,g=200,b=140} },
        { bind='title', x=34, y=9, size=16, color={r=240,g=225,b=232} },
        { bind='duration', right=12, y=9, size=15, color={r=180,g=150,b=165}, halign='right' },
      } }
