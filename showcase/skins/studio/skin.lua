-- ============================================================================
-- Studio Deck — baked brushed-metal mixer chrome (assets/studio.png) with the
-- engine drawing only the LIVE layer on top: LCD text, visualizer bars, the
-- playlist, and seek/volume fills; transparent hotspots over the baked knobs's
-- neighbouring buttons. One attached 720×480 panel. No gradient primitives.
-- ============================================================================

image{ asset="studio.png", x=0, y=0, w=720, h=480 }

region{ path=rect{x=2, y=2, w=716, h=476}, role='drag', on_press=function() host.begin_drag() end }
-- window close/minimize are real macOS traffic-light buttons, added by the app

-- LCD title bar (live text on the baked glass)
text{ value="track_title", x=36, y=30, size=18, color={r=141,g=255,b=173} }
text{ value="artist",      x=330, y=34, size=14, color={r=110,g=200,b=150} }
text{ value="clock", font="DSEG7Classic-Regular.ttf", x=600, y=34, size=13, color={r=95,g=175,b=135} }

-- visualizer bars on the baked screen
value_fill{ path=rect{x=44,  y=100, w=40, h=170}, value="viz_0", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=98,  y=100, w=40, h=170}, value="viz_1", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=152, y=100, w=40, h=170}, value="viz_2", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=206, y=100, w=40, h=170}, value="viz_3", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=260, y=100, w=40, h=170}, value="viz_4", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=314, y=100, w=40, h=170}, value="viz_5", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=368, y=100, w=40, h=170}, value="viz_6", direction='up', color={r=77,g=160,b=240} }
value_fill{ path=rect{x=422, y=100, w=40, h=170}, value="viz_7", direction='up', color={r=77,g=160,b=240} }

-- playlist in the recessed well
list{ collection="playlist", x=520, y=86, w=168, h=208, row_height=32,
      on_select="play_index", selected="current_index", highlight={r=36,g=112,b=66, a=200},
      template={
        { bind='now', x=8, y=8, size=14, color={r=92,g=255,b=154} },
        { bind='title', x=28, y=8, size=14, color={r=222,g=230,b=244} },
      } }

-- seek + volume fills over the baked slots
scrub{ value="volume",   on_seek="set_volume", x=274, y=326, w=210, h=8, direction='right', color={r=255,g=200,b=87} }
scrub{ value="position", on_seek="seek",        x=24, y=400, w=284, h=8, direction='right', color={r=92,g=255,b=154} }

-- transport hotspots over the baked buttons (prev / stop / play / next)
region{ path=rect{x=340, y=392, w=56, h=46}, on_press=function() host.prev() end }
region{ path=rect{x=406, y=392, w=56, h=46}, on_press=function() host.stop() end }
region{ path=rect{x=472, y=392, w=70, h=46}, on_press=function() host.toggle_play() end }
region{ path=rect{x=552, y=392, w=56, h=46}, on_press=function() host.next() end }
