-- Shaped body (rounded rect) over the transparent window; whole-body drag region.
fill{ path = rounded_rect{x=0, y=0, w=420, h=660, radius=18}, color = {r=18, g=22, b=32} }
region{ path = rounded_rect{x=0, y=0, w=420, h=660, radius=18}, role='drag',
        on_press = function() host.begin_drag() end }

-- window buttons
text{ text="_", x=384, y=8, size=16, color={r=200,g=200,b=210} }
region{ path=rect{x=380,y=8,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=402, y=8, size=16, color={r=230,g=140,b=140} }
region{ path=rect{x=398,y=8,w=16,h=18}, on_press=function() host.close() end }

-- now playing
text{ value="track_title", x=24, y=40, size=22, color={r=235,g=240,b=250} }
text{ value="artist", x=24, y=72, size=15, color={r=150,g=165,b=190} }
text{ value="time", x=24, y=96, size=13, color={r=120,g=135,b=160} }

-- seek scrub (position -> seek)
scrub{ value="position", on_seek="seek", x=24, y=128, w=372, h=10,
       direction='right', color={r=92,g=255,b=154} }

-- visualizer bars (viz_0..viz_11)
value_fill{ path=rect{x=24,  y=160, w=28, h=60}, value="viz_0", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=56,  y=160, w=28, h=60}, value="viz_1", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=88,  y=160, w=28, h=60}, value="viz_2", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=120, y=160, w=28, h=60}, value="viz_3", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=152, y=160, w=28, h=60}, value="viz_4", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=184, y=160, w=28, h=60}, value="viz_5", direction='up', color={r=77,g=215,b=255} }

-- transport row
fill{ path=rect{x=24,  y=240, w=80, h=44}, color={r=48,g=58,b=78},
      on_press=function() host.prev() end }
text{ text="prev", x=44, y=254, size=14, color={r=210,g=220,b=235} }
fill{ path=rect{x=112, y=240, w=110, h=44}, color={r=88,g=255,b=173},
      on_press=function() host.toggle_play() end }
text{ text="play/pause", x=124, y=254, size=13, color={r=8,g=30,b=18} }
fill{ path=rect{x=230, y=240, w=80, h=44}, color={r=48,g=58,b=78},
      on_press=function() host.next() end }
text{ text="next", x=250, y=254, size=14, color={r=210,g=220,b=235} }

-- volume scrub (volume -> set_volume)
text{ text="vol", x=24, y=300, size=13, color={r=150,g=165,b=190} }
scrub{ value="volume", on_seek="set_volume", x=64, y=302, w=332, h=10,
       direction='right', color={r=255,g=200,b=87} }

-- playlist
list{ collection="playlist", x=24, y=336, w=372, h=300, row_height=34,
      on_select="play_index", highlight={r=40,g=52,b=44}, selected="current_index",
      template={
        { bind='now', x=8, y=8, size=15, color={r=92,g=255,b=154} },
        { bind='title', x=32, y=8, size=15, color={r=225,g=232,b=245} },
        { bind='artist', x=210, y=8, size=15, color={r=150,g=165,b=190} },
        { bind='duration', right=10, y=8, size=14, color={r=140,g=155,b=180}, halign='right' },
      } }
