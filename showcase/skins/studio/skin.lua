-- ============================================================================
-- Studio Deck — one attached 720×480 app surface: LCD title bar, blue
-- visualizer, playlist + library column, mixer knobs, transport.
-- ============================================================================

-- edge-lit panel: light rim + inset gradient body
fill{ path = rounded_rect{x=0, y=0, w=720, h=480, radius=18}, color={r=150,g=168,b=200} }
fill{ path = rounded_rect{x=2, y=2, w=716, h=476, radius=16},
      gradient={ type='linear', from={x=0,y=0}, to={x=720,y=480},
        stops={ {at=0.0,color={r=38,g=50,b=74}}, {at=0.6,color={r=17,g=26,b=43}}, {at=1.0,color={r=10,g=15,b=26}} } } }
region{ path = rounded_rect{x=2, y=2, w=716, h=476, radius=16}, role='drag',
        on_press=function() host.begin_drag() end }

-- window buttons
text{ text="_", x=684, y=12, size=16, color={r=200,g=205,b=220} }
region{ path=rect{x=680,y=12,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=702, y=12, size=16, color={r=235,g=145,b=150} }
region{ path=rect{x=698,y=12,w=16,h=18}, on_press=function() host.close() end }

-- LCD title bar
fill{ path=rounded_rect{x=20, y=20, w=680, h=46, radius=9}, color={r=31,g=122,b=68} }
fill{ path=rounded_rect{x=22, y=22, w=676, h=42, radius=8}, color={r=6,g=20,b=13} }
text{ value="track_title", x=36, y=32, size=20, color={r=141,g=255,b=173} }
text{ value="artist", x=320, y=36, size=15, color={r=120,g=200,b=160} }
text{ value="time", x=620, y=36, size=14, color={r=95,g=175,b=135} }

-- visualizer panel (blue value_fill bars, viz_*)
fill{ path=rounded_rect{x=20, y=82, w=488, h=200, radius=11}, color={r=10,g=16,b=32} }
fill{ path=rounded_rect{x=20, y=82, w=488, h=200, radius=11}, color={r=49,g=65,b=91, a=0} }
value_fill{ path=rect{x=44,  y=100, w=44, h=164}, value="viz_0", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=100, y=100, w=44, h=164}, value="viz_1", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=156, y=100, w=44, h=164}, value="viz_2", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=212, y=100, w=44, h=164}, value="viz_3", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=268, y=100, w=44, h=164}, value="viz_4", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=324, y=100, w=44, h=164}, value="viz_5", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=380, y=100, w=44, h=164}, value="viz_6", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=436, y=100, w=44, h=164}, value="viz_7", direction='up', color={r=77,g=160,b=240} }

-- playlist + library column
fill{ path=rounded_rect{x=524, y=82, w=176, h=300, radius=11}, color={r=11,g=19,b=34} }
list{ collection="playlist", x=532, y=90, w=160, h=250, row_height=34,
      on_select="play_index", selected="current_index", highlight={r=36,g=112,b=66, a=200},
      template={
        { bind='now', x=8, y=8, size=14, color={r=92,g=255,b=154} },
        { bind='title', x=28, y=8, size=14, color={r=222,g=230,b=244} },
      } }
text{ text="Library: 184 tracks", x=536, y=352, size=13, color={r=125,g=139,b=161} }

-- mixer knobs (decoration): radial body + green pointer
fill{ path=circle{cx=60, cy=326, r=27}, gradient={ type='radial', center={x=60,y=316}, radius=32, stops={ {at=0.0,color={r=122,g=132,b=151}}, {at=1.0,color={r=36,g=44,b=59}} } } }
fill{ path=rect{x=58, y=304, w=4, h=16}, color={r=125,g=255,b=176} }
fill{ path=circle{cx=132, cy=326, r=27}, gradient={ type='radial', center={x=132,y=316}, radius=32, stops={ {at=0.0,color={r=122,g=132,b=151}}, {at=1.0,color={r=36,g=44,b=59}} } } }
fill{ path=rect{x=130, y=304, w=4, h=16}, color={r=125,g=255,b=176} }
fill{ path=circle{cx=204, cy=326, r=27}, gradient={ type='radial', center={x=204,y=316}, radius=32, stops={ {at=0.0,color={r=122,g=132,b=151}}, {at=1.0,color={r=36,g=44,b=59}} } } }
fill{ path=rect{x=202, y=304, w=4, h=16}, color={r=125,g=255,b=176} }

-- volume slider (right of knobs)
text{ text="vol", x=250, y=306, size=13, color={r=150,g=165,b=190} }
fill{ path=rounded_rect{x=286, y=320, w=200, h=12, radius=6}, color={r=27,g=36,b=52} }
scrub{ value="volume", on_seek="set_volume", x=286, y=320, w=200, h=12, direction='right', color={r=255,g=200,b=87} }

-- seek + transport (lower)
fill{ path=rounded_rect{x=20, y=372, w=488, h=12, radius=6}, color={r=27,g=36,b=52} }
scrub{ value="position", on_seek="seek", x=20, y=372, w=488, h=12, direction='right', color={r=92,g=255,b=154} }
fill{ path=rounded_rect{x=20,  y=404, w=88, h=48, radius=10}, gradient={ type='linear', from={x=0,y=404}, to={x=0,y=452}, stops={ {at=0.0,color={r=47,g=58,b=81}}, {at=1.0,color={r=28,g=36,b=51}} } }, on_press=function() host.prev() end }
text{ text="<<", x=54, y=420, size=16, color={r=211,g=219,b=234} }
fill{ path=rounded_rect{x=118, y=404, w=88, h=48, radius=10}, gradient={ type='linear', from={x=0,y=404}, to={x=0,y=452}, stops={ {at=0.0,color={r=47,g=58,b=81}}, {at=1.0,color={r=28,g=36,b=51}} } }, on_press=function() host.stop() end }
text{ text="[]", x=156, y=420, size=15, color={r=211,g=219,b=234} }
fill{ path=rounded_rect{x=216, y=404, w=112, h=48, radius=10}, gradient={ type='linear', from={x=0,y=404}, to={x=0,y=452}, stops={ {at=0.0,color={r=120,g=255,b=173}}, {at=1.0,color={r=22,g=138,b=76}} } }, on_press=function() host.toggle_play() end }
text{ text="play", x=252, y=420, size=15, color={r=7,g=48,b=27} }
fill{ path=rounded_rect{x=338, y=404, w=88, h=48, radius=10}, gradient={ type='linear', from={x=0,y=404}, to={x=0,y=452}, stops={ {at=0.0,color={r=47,g=58,b=81}}, {at=1.0,color={r=28,g=36,b=51}} } }, on_press=function() host.next() end }
text{ text=">>", x=372, y=420, size=16, color={r=211,g=219,b=234} }
