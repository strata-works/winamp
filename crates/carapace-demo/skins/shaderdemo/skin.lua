-- Full-canvas animated WGSL background (renders UNDER the 2D UI).
-- `intensity` is a baked literal; `hue` is a host binding key (playback position) resolved
-- every frame, so the color ramp shifts as the demo's music plays. `u.time` always animates.
shader{ src = "bg.wgsl", x = 0, y = 0, w = 480, h = 320,
        uniforms = { hue = "position", intensity = 1.0 } }

-- 2D UI drawn on top of the shader background — proves background-layer compositing.
fill{ path = rounded_rect{x = 20, y = 20, w = 260, h = 60, radius = 12},
      color = {r = 12, g = 12, b = 18, a = 210} }
text{ text = "shader{} live", x = 36, y = 30, size = 22, color = {r = 244, g = 244, b = 250} }
text{ text = "WGSL background layer under 2D UI", x = 36, y = 58, size = 11,
      color = {r = 176, g = 182, b = 200} }
