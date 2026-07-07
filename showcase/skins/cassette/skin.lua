-- ============================================================================
-- Cassette — a realistic tape resting on a control deck (600×400).
-- The tape body is a baked PNG (plastic, reels, label, screws — full detail).
-- The engine draws only the LIVE layer: title/artist on the label, the seek
-- scrub, and the deck's transport keys. Playlist is omitted by design (the
-- object metaphor: prev/next change tracks).
-- ============================================================================

-- deck base (drawn first; the tape sits on top of it)
fill{ path = rounded_rect{x=0, y=322, w=600, h=78, radius=20},
      gradient={ type='linear', from={x=0,y=322}, to={x=0,y=400},
        stops={ {at=0.0,color={r=44,g=46,b=52}}, {at=1.0,color={r=16,g=17,b=21}} } } }

-- realistic cassette body (baked image), 0..340 — overlaps the deck top
image{ asset="cassette.png", x=0, y=0, w=600, h=340 }
region{ path=rect{x=0, y=0, w=600, h=322}, role='drag', on_press=function() host.begin_drag() end }
-- window close/minimize are real macOS traffic-light buttons, added by the app

-- live label text (dark ink on the cream paper label)
text{ value="track_title", x=90, y=44, size=21, color={r=42,g=33,b=24} }
text{ value="artist", x=90, y=72, size=14, color={r=112,g=94,b=68} }

-- seek scrub on the deck
fill{ path=rounded_rect{x=40, y=346, w=520, h=8, radius=4}, color={r=30,g=23,b=16} }
scrub{ value="position", on_seek="seek", x=40, y=346, w=520, h=8, direction='right', color={r=255,g=176,b=112} }

-- deck transport keys: prev / play / stop / next
fill{ path=rounded_rect{x=150, y=364, w=70, h=30, radius=6},
      gradient={ type='linear', from={x=0,y=364}, to={x=0,y=394}, stops={ {at=0.0,color={r=214,g=184,b=142}}, {at=1.0,color={r=150,g=118,b=78}} } },
      on_press=function() host.prev() end }
text{ text="<<", x=171, y=372, size=14, color={r=58,g=40,b=27} }
fill{ path=rounded_rect{x=228, y=364, w=88, h=30, radius=6},
      gradient={ type='linear', from={x=0,y=364}, to={x=0,y=394}, stops={ {at=0.0,color={r=120,g=255,b=173}}, {at=1.0,color={r=22,g=138,b=76}} } },
      on_press=function() host.toggle_play() end }
text{ text="> PLAY", x=246, y=372, size=13, color={r=7,g=48,b=27} }
fill{ path=rounded_rect{x=324, y=364, w=70, h=30, radius=6},
      gradient={ type='linear', from={x=0,y=364}, to={x=0,y=394}, stops={ {at=0.0,color={r=214,g=184,b=142}}, {at=1.0,color={r=150,g=118,b=78}} } },
      on_press=function() host.stop() end }
text{ text="[]", x=350, y=372, size=13, color={r=58,g=40,b=27} }
fill{ path=rounded_rect{x=402, y=364, w=70, h=30, radius=6},
      gradient={ type='linear', from={x=0,y=364}, to={x=0,y=394}, stops={ {at=0.0,color={r=214,g=184,b=142}}, {at=1.0,color={r=150,g=118,b=78}} } },
      on_press=function() host.next() end }
text{ text=">>", x=423, y=372, size=14, color={r=58,g=40,b=27} }
