-- ============================================================================
-- Faceplate — two detached components over the transparent 380×560 window:
--   (1) a shaped, edge-lit body (LCD + seek + transport + volume)
--   (2) a separate queue drawer, below, with a transparent gap between them.
-- Both panels are draggable; the gap shows the desktop through.
-- ============================================================================

-- ---- Component 1: faceplate body (edge-lit): light rim rrect + inset gradient body
fill{ path = rounded_rect{x=0, y=0, w=380, h=300, radius=30}, color = {r=150, g=161, b=193} }
fill{ path = rounded_rect{x=2, y=2, w=376, h=296, radius=28},
      gradient = { type='linear', from={x=0,y=0}, to={x=0,y=300},
        stops={ {at=0.0, color={r=51,g=45,b=85}}, {at=0.55, color={r=23,g=27,b=45}}, {at=1.0, color={r=10,g=14,b=23}} } } }
-- radial glow, top-left
fill{ path = circle{cx=64, cy=60, r=110},
      gradient = { type='radial', center={x=64,y=60}, radius=110,
        stops={ {at=0.0, color={r=255,g=255,b=255, a=34}}, {at=1.0, color={r=255,g=255,b=255, a=0}} } } }
-- whole-body drag (declared before controls so controls win hit-test)
region{ path = rounded_rect{x=2, y=2, w=376, h=296, radius=28}, role='drag',
        on_press = function() host.begin_drag() end }

-- window buttons
text{ text="_", x=340, y=10, size=16, color={r=200,g=205,b=220} }
region{ path=rect{x=336,y=10,w=16,h=18}, on_press=function() host.minimize() end }
text{ text="x", x=360, y=10, size=16, color={r=235,g=145,b=150} }
region{ path=rect{x=356,y=10,w=16,h=18}, on_press=function() host.close() end }

-- LCD: green rim + inset dark-green ground
fill{ path = rounded_rect{x=24, y=44, w=332, h=120, radius=11}, color={r=31,g=122,b=68} }
fill{ path = rounded_rect{x=26, y=46, w=328, h=116, radius=9},  color={r=4,g=20,b=11} }
text{ value="track_title", x=42, y=60, size=23,
      gradient={ type='linear', from={x=0,y=0}, to={x=0,y=24},
        stops={ {at=0.0, color={r=141,g=255,b=173}}, {at=1.0, color={r=76,g=204,b=126}} } } }
text{ value="artist", x=42, y=94, size=15, color={r=95,g=191,b=126} }
text{ value="time", x=42, y=134, size=14, color={r=79,g=175,b=125} }
-- LCD spectrum (viz_*) bottom-right of the panel
value_fill{ path=rect{x=272, y=132, w=6, h=22}, value="viz_0", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=283, y=132, w=6, h=22}, value="viz_1", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=294, y=132, w=6, h=22}, value="viz_2", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=305, y=132, w=6, h=22}, value="viz_3", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=316, y=132, w=6, h=22}, value="viz_4", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=327, y=132, w=6, h=22}, value="viz_5", direction='up', color={r=141,g=255,b=173} }

-- seek
fill{ path=rounded_rect{x=24, y=182, w=332, h=12, radius=6}, color={r=27,g=36,b=52} }
scrub{ value="position", on_seek="seek", x=24, y=182, w=332, h=12, direction='right', color={r=92,g=255,b=154} }

-- transport row: prev / stop / play / next + volume
fill{ path=rounded_rect{x=24,  y=214, w=52, h=46, radius=10}, gradient={ type='linear', from={x=0,y=214}, to={x=0,y=260}, stops={ {at=0.0,color={r=47,g=58,b=81}}, {at=1.0,color={r=28,g=36,b=51}} } },
      on_press=function() host.prev() end }
text{ text="<<", x=40, y=228, size=15, color={r=211,g=219,b=234} }
fill{ path=rounded_rect{x=84,  y=214, w=52, h=46, radius=10}, gradient={ type='linear', from={x=0,y=214}, to={x=0,y=260}, stops={ {at=0.0,color={r=47,g=58,b=81}}, {at=1.0,color={r=28,g=36,b=51}} } },
      on_press=function() host.stop() end }
text{ text="[]", x=102, y=228, size=14, color={r=211,g=219,b=234} }
fill{ path=rounded_rect{x=144, y=214, w=64, h=46, radius=10}, gradient={ type='linear', from={x=0,y=214}, to={x=0,y=260}, stops={ {at=0.0,color={r=120,g=255,b=173}}, {at=1.0,color={r=22,g=138,b=76}} } },
      on_press=function() host.toggle_play() end }
text{ text=">", x=172, y=227, size=18, color={r=7,g=48,b=27} }
fill{ path=rounded_rect{x=216, y=214, w=52, h=46, radius=10}, gradient={ type='linear', from={x=0,y=214}, to={x=0,y=260}, stops={ {at=0.0,color={r=47,g=58,b=81}}, {at=1.0,color={r=28,g=36,b=51}} } },
      on_press=function() host.next() end }
text{ text=">>", x=232, y=228, size=15, color={r=211,g=219,b=234} }
-- volume
fill{ path=rounded_rect{x=280, y=228, w=76, h=10, radius=5}, color={r=27,g=36,b=52} }
scrub{ value="volume", on_seek="set_volume", x=280, y=228, w=76, h=10, direction='right', color={r=255,g=200,b=87} }

-- ---- transparent gap (y 300..320) ----

-- ---- Component 2: detached queue drawer (edge + panel)
fill{ path = rounded_rect{x=8, y=320, w=364, h=232, radius=17}, color={r=90,g=101,b=130} }
fill{ path = rounded_rect{x=10, y=322, w=360, h=228, radius=15},
      gradient={ type='linear', from={x=0,y=322}, to={x=0,y=550}, stops={ {at=0.0,color={r=20,g=30,b=48}}, {at=1.0,color={r=11,g=17,b=32}} } } }
region{ path = rounded_rect{x=10, y=322, w=360, h=228, radius=15}, role='drag',
        on_press = function() host.begin_drag() end }
text{ text="QUEUE", x=24, y=332, size=11, color={r=120,g=134,b=160} }
list{ collection="playlist", x=22, y=352, w=336, h=190, row_height=36,
      on_select="play_index", selected="current_index", highlight={r=40,g=52,b=44, a=210},
      template={
        { bind='now', x=10, y=9, size=15, color={r=92,g=255,b=154} },
        { bind='title', x=34, y=9, size=15, color={r=225,g=232,b=245} },
        { bind='duration', right=12, y=9, size=14, color={r=140,g=155,b=180}, halign='right' },
      } }
