-- ============================================================================
-- Faceplate — baked anodized-aluminum chrome (assets/faceplate.png) with the
-- engine drawing only the LIVE layer on top. Two detached components (player
-- body + queue drawer) over the transparent 380×560 window; the gap shows the
-- desktop through. No gradient primitives — all depth is baked into the image.
-- ============================================================================

image{ asset="faceplate.png", x=0, y=0, w=380, h=560 }

-- drag both panels (window close/minimize are real macOS traffic-light buttons, added by the app)
region{ path=rect{x=2,  y=2,   w=376, h=296}, role='drag', on_press=function() host.begin_drag() end }
region{ path=rect{x=10, y=322, w=360, h=228}, role='drag', on_press=function() host.begin_drag() end }

-- LCD live text (green phosphor) on the baked glass
text{ value="track_title", x=44, y=62, size=22, color={r=141,g=255,b=173} }
text{ value="artist",      x=44, y=98, size=15, color={r=95,g=200,b=140} }
text{ value="time",        x=44, y=130, size=14, color={r=82,g=180,b=128} }
value_fill{ path=rect{x=272, y=130, w=6, h=22}, value="viz_0", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=283, y=130, w=6, h=22}, value="viz_1", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=294, y=130, w=6, h=22}, value="viz_2", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=305, y=130, w=6, h=22}, value="viz_3", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=316, y=130, w=6, h=22}, value="viz_4", direction='up', color={r=141,g=255,b=173} }
value_fill{ path=rect{x=327, y=130, w=6, h=22}, value="viz_5", direction='up', color={r=141,g=255,b=173} }

-- seek + volume fills over the baked recessed slots
scrub{ value="position",  on_seek="seek",       x=28, y=178, w=324, h=8, direction='right', color={r=92,g=255,b=154} }
scrub{ value="volume",    on_seek="set_volume", x=26, y=271, w=294, h=7, direction='right', color={r=255,g=200,b=87} }

-- transport hotspots over the baked buttons (prev / stop / play / next)
region{ path=rect{x=22,  y=210, w=50, h=50}, on_press=function() host.prev() end }
region{ path=rect{x=82,  y=210, w=50, h=50}, on_press=function() host.stop() end }
region{ path=rect{x=142, y=210, w=62, h=50}, on_press=function() host.toggle_play() end }
region{ path=rect{x=214, y=210, w=50, h=50}, on_press=function() host.next() end }

-- queue list in the baked well
list{ collection="playlist", x=32, y=366, w=316, h=164, row_height=34,
      on_select="play_index", selected="current_index", highlight={r=52,g=68,b=58, a=210},
      template={
        { bind='now', x=8, y=8, size=15, color={r=92,g=255,b=154} },
        { bind='title', x=32, y=8, size=15, color={r=225,g=232,b=245} },
        { bind='duration', right=12, y=8, size=14, color={r=140,g=155,b=180}, halign='right' },
      } }
