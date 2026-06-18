-- body silhouette (green organic head + wings), traced as a filled blob
fill{ path = {
  {x=70,y=10},{x=272,y=10},{x=300,y=40},{x=332,y=70},{x=332,y=230},{x=300,y=250},
  {x=300,y=300},{x=240,y=370},{x=171,y=392},{x=102,y=370},{x=42,y=300},{x=42,y=250},
  {x=10,y=230},{x=10,y=70},{x=42,y=40}
}, color = {r=86,g=196,b=40} }

-- bottom "face" region (flat placeholder; real photo is Phase 5)
fill{ path = {{x=70,y=250},{x=272,y=250},{x=240,y=370},{x=171,y=392},{x=102,y=370}},
      color = {r=58,g=132,b=36} }

-- black display screen
fill{ path = {{x=72,y=56},{x=270,y=56},{x=270,y=206},{x=72,y=206}}, color = {r=8,g=8,b=10} }

-- 6 speaker grilles as octagons (left wing x~37, right wing x~305; y 100/152/204)
fill{ path = {{x=22,y=92},{x=52,y=92},{x=66,y=106},{x=66,y=130},{x=52,y=144},{x=22,y=144},{x=8,y=130},{x=8,y=106}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=22,y=144},{x=52,y=144},{x=66,y=158},{x=66,y=182},{x=52,y=196},{x=22,y=196},{x=8,y=182},{x=8,y=158}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=22,y=196},{x=52,y=196},{x=66,y=210},{x=66,y=234},{x=52,y=248},{x=22,y=248},{x=8,y=234},{x=8,y=210}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=290,y=92},{x=320,y=92},{x=334,y=106},{x=334,y=130},{x=320,y=144},{x=290,y=144},{x=276,y=130},{x=276,y=106}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=290,y=144},{x=320,y=144},{x=334,y=158},{x=334,y=182},{x=320,y=196},{x=290,y=196},{x=276,y=182},{x=276,y=158}}, color = {r=150,g=170,b=150} }
fill{ path = {{x=290,y=196},{x=320,y=196},{x=334,y=210},{x=334,y=234},{x=320,y=248},{x=290,y=248},{x=276,y=234},{x=276,y=210}}, color = {r=150,g=170,b=150} }

-- transport row (play -> toggle_play, stop -> stop), drawn + hotspot each
region{ path = {{x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48}}, on_press = function() host.toggle_play() end }
fill{ path = {{x=150,y=24},{x=178,y=24},{x=178,y=48},{x=150,y=48}}, color = {r=200,g=235,b=200} }
region{ path = {{x=184,y=24},{x=212,y=24},{x=212,y=48},{x=184,y=48}}, on_press = function() host.stop() end }
fill{ path = {{x=184,y=24},{x=212,y=24},{x=212,y=48},{x=184,y=48}}, color = {r=200,g=235,b=200} }

-- sunburst options button (top-right) as a diamond
fill{ path = {{x=300,y=24},{x=320,y=44},{x=300,y=64},{x=280,y=44}}, color = {r=235,g=240,b=120} }

-- side arrows
fill{ path = {{x=8,y=160},{x=24,y=150},{x=24,y=170}}, color = {r=120,g=210,b=70} }
fill{ path = {{x=334,y=160},{x=318,y=150},{x=318,y=170}}, color = {r=120,g=210,b=70} }

-- seek bar bound to position
value_fill{ path = {{x=78,y=216},{x=264,y=216},{x=264,y=230},{x=78,y=230}},
            value = "position", color = {r=120,g=230,b=80} }

-- center logo button
fill{ path = {{x=156,y=236},{x=186,y=236},{x=186,y=256},{x=156,y=256}}, color = {r=40,g=120,b=30} }
