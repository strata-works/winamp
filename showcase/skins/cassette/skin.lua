-- cassette body + drag
fill{ path = rounded_rect{x=0, y=0, w=600, h=400, radius=32},
      gradient={ type='linear', from={x=0,y=0}, to={x=600,y=400},
        stops={ {at=0.0, color={r=58,g=40,b=27}}, {at=0.45, color={r=23,g=16,b=11}}, {at=1.0, color={r=16,g=24,b=39}} } } }
region{ path = rounded_rect{x=0, y=0, w=600, h=400, radius=32}, role='drag', on_press=function() host.begin_drag() end }
text{ text="_", x=560, y=12, size=16, color={r=220,g=205,b=185} }
region{ path=rect{x=556,y=12,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=578, y=12, size=16, color={r=235,g=150,b=150} }
region{ path=rect{x=574,y=12,w=16,h=18}, on_press=function() host.close() end }

-- tape label
fill{ path=rounded_rect{x=140, y=40, w=320, h=70, radius=6},
      gradient={ type='linear', from={x=0,y=40}, to={x=0,y=110}, stops={ {at=0.0,color={r=255,g=214,b=107}}, {at=1.0,color={r=216,g=155,b=37}} } } }
text{ value="track_title", x=160, y=54, size=20, color={r=58,g=40,b=27} }
text{ value="artist", x=160, y=82, size=15, color={r=110,g=78,b=45} }

-- reels (sweep-gradient spokes + hub)
fill{ path=circle{cx=170, cy=250, r=64},
      gradient={ type='sweep', center={x=170,y=250}, start_deg=0, end_deg=360,
        stops={ {at=0.0,color={r=40,g=44,b=54}}, {at=0.25,color={r=17,g=24,b=39}}, {at=0.5,color={r=40,g=44,b=54}}, {at=0.75,color={r=17,g=24,b=39}}, {at=1.0,color={r=40,g=44,b=54}} } } }
fill{ path=circle{cx=170, cy=250, r=22}, color={r=217,g=168,b=92} }
fill{ path=circle{cx=170, cy=250, r=8}, color={r=15,g=23,b=42} }
fill{ path=circle{cx=430, cy=250, r=64},
      gradient={ type='sweep', center={x=430,y=250}, start_deg=0, end_deg=360,
        stops={ {at=0.0,color={r=40,g=44,b=54}}, {at=0.25,color={r=17,g=24,b=39}}, {at=0.5,color={r=40,g=44,b=54}}, {at=0.75,color={r=17,g=24,b=39}}, {at=1.0,color={r=40,g=44,b=54}} } } }
fill{ path=circle{cx=430, cy=250, r=22}, color={r=217,g=168,b=92} }
fill{ path=circle{cx=430, cy=250, r=8}, color={r=15,g=23,b=42} }

-- tape window between reels + time
fill{ path=rounded_rect{x=250, y=150, w=100, h=40, radius=4}, color={r=7,g=20,b=13} }
text{ value="time", x=262, y=162, size=13, color={r=141,g=255,b=173} }

-- VU meter (music-reactive; binds viz_*) above the window, cassette-deck style
value_fill{ path=rect{x=256, y=116, w=8, h=28}, value="viz_0", direction='up', color={r=255,g=200,b=120} }
value_fill{ path=rect{x=271, y=116, w=8, h=28}, value="viz_1", direction='up', color={r=255,g=200,b=120} }
value_fill{ path=rect{x=286, y=116, w=8, h=28}, value="viz_2", direction='up', color={r=255,g=190,b=110} }
value_fill{ path=rect{x=301, y=116, w=8, h=28}, value="viz_3", direction='up', color={r=255,g=180,b=100} }
value_fill{ path=rect{x=316, y=116, w=8, h=28}, value="viz_4", direction='up', color={r=255,g=170,b=90} }
value_fill{ path=rect{x=331, y=116, w=8, h=28}, value="viz_5", direction='up', color={r=255,g=160,b=80} }

-- keys (prev / play / stop / next)
fill{ path=rounded_rect{x=180, y=326, w=54, h=40, radius=6}, color={r=210,g=180,b=140}, on_press=function() host.prev() end }
text{ text="<<", x=196, y=338, size=14, color={r=58,g=40,b=27} }
fill{ path=rounded_rect{x=240, y=326, w=54, h=40, radius=6},
      gradient={ type='linear', from={x=0,y=326}, to={x=0,y=366}, stops={ {at=0.0,color={r=120,g=255,b=173}}, {at=1.0,color={r=22,g=138,b=76}} } },
      on_press=function() host.toggle_play() end }
text{ text=">", x=262, y=338, size=16, color={r=8,g=30,b=18} }
fill{ path=rounded_rect{x=300, y=326, w=54, h=40, radius=6}, color={r=210,g=180,b=140}, on_press=function() host.stop() end }
text{ text="[]", x=318, y=338, size=13, color={r=58,g=40,b=27} }
fill{ path=rounded_rect{x=360, y=326, w=54, h=40, radius=6}, color={r=210,g=180,b=140}, on_press=function() host.next() end }
text{ text=">>", x=376, y=338, size=14, color={r=58,g=40,b=27} }

-- volume (small slider upper-right, above the right reel)
text{ text="vol", x=384, y=102, size=12, color={r=210,g=180,b=130} }
fill{ path=rounded_rect{x=412, y=116, w=140, h=8, radius=4}, color={r=40,g=30,b=20} }
scrub{ value="volume", on_seek="set_volume", x=412, y=116, w=140, h=8, direction='right', color={r=255,g=200,b=120} }

-- slim seek
fill{ path=rounded_rect{x=100, y=300, w=400, h=8, radius=4}, color={r=40,g=30,b=20} }
scrub{ value="position", on_seek="seek", x=100, y=300, w=400, h=8, direction='right', color={r=255,g=170,b=120} }
