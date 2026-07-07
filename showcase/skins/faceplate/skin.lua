-- shaped body + radial glow; whole-body drag (declared first so controls win hit-test)
fill{ path = rounded_rect{x=0, y=0, w=380, h=560, radius=30},
      gradient = { type='linear', from={x=0,y=0}, to={x=0,y=560},
        stops={ {at=0.0, color={r=48,g=44,b=84}}, {at=0.5, color={r=21,g=25,b=42}}, {at=1.0, color={r=8,g=11,b=18}} } } }
fill{ path = circle{cx=70, cy=70, r=120},
      gradient = { type='radial', center={x=70,y=70}, radius=120,
        stops={ {at=0.0, color={r=255,g=255,b=255, a=40}}, {at=1.0, color={r=255,g=255,b=255, a=0}} } } }
region{ path = rounded_rect{x=0, y=0, w=380, h=560, radius=30}, role='drag',
        on_press = function() host.begin_drag() end }

-- window buttons
text{ text="_", x=344, y=10, size=16, color={r=200,g=205,b=220} }
region{ path=rect{x=340,y=10,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=362, y=10, size=16, color={r=235,g=145,b=150} }
region{ path=rect{x=358,y=10,w=16,h=18}, on_press=function() host.close() end }

-- LCD panel
fill{ path = rounded_rect{x=24, y=44, w=332, h=132, radius=10}, color={r=3,g=16,b=8} }
fill{ path = rounded_rect{x=24, y=44, w=332, h=132, radius=10}, color={r=31,g=122,b=68, a=40} }
text{ value="track_title", x=40, y=62, size=22,
      gradient={ type='linear', from={x=0,y=0}, to={x=0,y=24},
        stops={ {at=0.0, color={r=141,g=255,b=173}}, {at=1.0, color={r=60,g=200,b=120}} } } }
text{ value="artist", x=40, y=96, size=15, color={r=110,g=200,b=150} }
text{ value="time", x=40, y=140, size=14, color={r=90,g=175,b=125} }

-- spectrum visualizer (viz_0..viz_5), tucked into the LCD's bottom-right corner
value_fill{ path=rect{x=236, y=146, w=8, h=24}, value="viz_0", direction='up', color={r=92,g=255,b=154} }
value_fill{ path=rect{x=256, y=146, w=8, h=24}, value="viz_1", direction='up', color={r=92,g=255,b=154} }
value_fill{ path=rect{x=276, y=146, w=8, h=24}, value="viz_2", direction='up', color={r=92,g=255,b=154} }
value_fill{ path=rect{x=296, y=146, w=8, h=24}, value="viz_3", direction='up', color={r=92,g=255,b=154} }
value_fill{ path=rect{x=316, y=146, w=8, h=24}, value="viz_4", direction='up', color={r=92,g=255,b=154} }
value_fill{ path=rect{x=336, y=146, w=8, h=24}, value="viz_5", direction='up', color={r=92,g=255,b=154} }

-- seek scrub
fill{ path=rounded_rect{x=24, y=190, w=332, h=12, radius=6}, color={r=31,g=41,b=55} }
scrub{ value="position", on_seek="seek", x=24, y=190, w=332, h=12, direction='right', color={r=92,g=255,b=154} }

-- transport row (prev / stop / play / next)
fill{ path=rounded_rect{x=40,  y=222, w=64, h=48, radius=8}, color={r=48,g=58,b=78},
      on_press=function() host.prev() end }
text{ text="<<", x=60, y=236, size=16, color={r=210,g=220,b=235} }
fill{ path=rounded_rect{x=112, y=222, w=64, h=48, radius=8}, color={r=48,g=58,b=78},
      on_press=function() host.stop() end }
text{ text="[]", x=134, y=236, size=15, color={r=210,g=220,b=235} }
fill{ path=rounded_rect{x=184, y=222, w=72, h=48, radius=8},
      gradient={ type='linear', from={x=0,y=222}, to={x=0,y=270},
        stops={ {at=0.0, color={r=120,g=255,b=173}}, {at=1.0, color={r=22,g=138,b=76}} } },
      on_press=function() host.toggle_play() end }
text{ text=">", x=214, y=236, size=18, color={r=8,g=30,b=18} }
fill{ path=rounded_rect{x=264, y=222, w=64, h=48, radius=8}, color={r=48,g=58,b=78},
      on_press=function() host.next() end }
text{ text=">>", x=284, y=236, size=16, color={r=210,g=220,b=235} }

-- volume
text{ text="vol", x=24, y=288, size=13, color={r=150,g=165,b=190} }
fill{ path=rounded_rect{x=64, y=290, w=292, h=10, radius=5}, color={r=31,g=41,b=55} }
scrub{ value="volume", on_seek="set_volume", x=64, y=290, w=292, h=10, direction='right', color={r=255,g=200,b=87} }

-- queue drawer
fill{ path=rounded_rect{x=24, y=320, w=332, h=224, radius=12}, color={r=13,g=18,b=30} }
list{ collection="playlist", x=34, y=330, w=312, h=204, row_height=34,
      on_select="play_index", selected="current_index", highlight={r=40,g=52,b=44, a=200},
      template={
        { bind='now', x=8, y=8, size=15, color={r=92,g=255,b=154} },
        { bind='title', x=32, y=8, size=15, color={r=225,g=232,b=245} },
        { bind='artist', x=170, y=8, size=14, color={r=150,g=165,b=190} },
        { bind='duration', right=10, y=8, size=14, color={r=140,g=155,b=180}, halign='right' },
      } }
