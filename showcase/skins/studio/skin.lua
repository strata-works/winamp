fill{ path = rounded_rect{x=0, y=0, w=720, h=480, radius=16},
      gradient={ type='linear', from={x=0,y=0}, to={x=720,y=480},
        stops={ {at=0.0, color={r=38,g=48,b=68}}, {at=0.6, color={r=16,g=24,b=39}}, {at=1.0, color={r=9,g=13,b=22}} } } }
region{ path = rounded_rect{x=0, y=0, w=720, h=480, radius=16}, role='drag',
        on_press=function() host.begin_drag() end }

text{ text="_", x=684, y=10, size=16, color={r=200,g=205,b=220} }
region{ path=rect{x=680,y=10,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=702, y=10, size=16, color={r=235,g=145,b=150} }
region{ path=rect{x=698,y=10,w=16,h=18}, on_press=function() host.close() end }

-- title bar
fill{ path=rounded_rect{x=20, y=20, w=680, h=44, radius=8}, color={r=7,g=20,b=13} }
text{ value="track_title", x=34, y=30, size=20, color={r=141,g=255,b=173} }
text{ value="artist", x=300, y=34, size=15, color={r=150,g=165,b=190} }
text{ value="time", x=580, y=34, size=14, color={r=120,g=135,b=160}, halign='right' }

-- visualizer (left column)
fill{ path=rounded_rect{x=20, y=80, w=340, h=180, radius=10}, color={r=10,g=16,b=32} }
value_fill{ path=rect{x=36,  y=100, w=40, h=140}, value="viz_0", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=84,  y=100, w=40, h=140}, value="viz_1", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=132, y=100, w=40, h=140}, value="viz_2", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=180, y=100, w=40, h=140}, value="viz_3", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=228, y=100, w=40, h=140}, value="viz_4", direction='up', color={r=77,g=215,b=255} }
value_fill{ path=rect{x=276, y=100, w=40, h=140}, value="viz_5", direction='up', color={r=77,g=215,b=255} }

-- knobs (decoration) + volume
fill{ path=circle{cx=60, cy=310, r=26}, gradient={ type='radial', center={x=60,y=302}, radius=30,
        stops={ {at=0.0, color={r=120,g=130,b=150}}, {at=1.0, color={r=31,g=41,b=55}} } } }
fill{ path=circle{cx=130, cy=310, r=26}, gradient={ type='radial', center={x=130,y=302}, radius=30,
        stops={ {at=0.0, color={r=120,g=130,b=150}}, {at=1.0, color={r=31,g=41,b=55}} } } }
text{ text="vol", x=180, y=286, size=13, color={r=150,g=165,b=190} }
fill{ path=rounded_rect{x=180, y=304, w=180, h=12, radius=6}, color={r=31,g=41,b=55} }
scrub{ value="volume", on_seek="set_volume", x=180, y=304, w=180, h=12, direction='right', color={r=255,g=200,b=87} }

-- seek + transport
fill{ path=rounded_rect{x=20, y=350, w=340, h=12, radius=6}, color={r=31,g=41,b=55} }
scrub{ value="position", on_seek="seek", x=20, y=350, w=340, h=12, direction='right', color={r=92,g=255,b=154} }
fill{ path=rounded_rect{x=20,  y=380, w=70, h=44, radius=8}, color={r=48,g=58,b=78}, on_press=function() host.prev() end }
text{ text="<<", x=44, y=394, size=15, color={r=210,g=220,b=235} }
fill{ path=rounded_rect{x=100, y=380, w=90, h=44, radius=8},
      gradient={ type='linear', from={x=0,y=380}, to={x=0,y=424}, stops={ {at=0.0,color={r=120,g=255,b=173}}, {at=1.0,color={r=22,g=138,b=76}} } },
      on_press=function() host.toggle_play() end }
text{ text="play", x=126, y=394, size=15, color={r=8,g=30,b=18} }
fill{ path=rounded_rect{x=200, y=380, w=70, h=44, radius=8}, color={r=48,g=58,b=78}, on_press=function() host.next() end }
text{ text=">>", x=224, y=394, size=15, color={r=210,g=220,b=235} }

-- playlist (right column)
fill{ path=rounded_rect{x=380, y=80, w=320, h=380, radius=10}, color={r=11,g=18,b=30} }
list{ collection="playlist", x=390, y=90, w=300, h=340, row_height=34,
      on_select="play_index", selected="current_index", highlight={r=36,g=112,b=66, a=200},
      template={
        { bind='now', x=8, y=8, size=15, color={r=92,g=255,b=154} },
        { bind='title', x=32, y=8, size=15, color={r=225,g=232,b=245} },
        { bind='duration', right=10, y=8, size=14, color={r=140,g=155,b=180}, halign='right' },
      } }
