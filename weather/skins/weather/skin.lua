local W, H = 400, 680

-- Animated WGSL background (renders UNDER the 2D UI). Host-bound numeric uniforms.
shader{ src = "weather.wgsl", x = 0, y = 0, w = W, h = H,
        uniforms = { condition = "wx_condition", is_day = "wx_is_day",
                     temp = "wx_temp", intensity = "wx_intensity", season = "wx_season" } }

-- Whole-window drag (the skin IS the window). Controls drawn later win hit-testing.
region{ path = rect{ x = 0, y = 0, w = W, h = H }, role = "drag",
        on_press = function() host.begin_drag() end }

-- Hero block.
text{ value = "location", x = 28, y = 40, size = 26, color = { r = 245, g = 247, b = 252 } }
text{ value = "condition_text", x = 28, y = 74, size = 14, color = { r = 210, g = 216, b = 230 } }
text{ value = "temp_now", x = 28, y = 108, size = 72, color = { r = 255, g = 255, b = 255 } }
text{ value = "hi_lo", x = 30, y = 196, size = 14, color = { r = 225, g = 230, b = 240 } }
text{ value = "feels", x = 160, y = 196, size = 14, color = { r = 225, g = 230, b = 240 } }

-- Horizontal hourly strip: 12 cells (time above, temp below), evenly spaced.
local hourly_y = 250
local n = 12
local pad = 20
local step = (W - pad * 2) / n
for i = 0, n - 1 do
  local cx = pad + step * i + step / 2
  text{ value = "wx_hour_" .. i .. "_time", x = cx, y = hourly_y, size = 11,
        halign = "center", color = { r = 205, g = 212, b = 226 } }
  text{ value = "wx_hour_" .. i .. "_temp", x = cx, y = hourly_y + 20, size = 13,
        halign = "center", color = { r = 245, g = 247, b = 252 } }
end

-- Vertical daily forecast list (collection = "daily"). Ends above the shader's bottom
-- silhouette band (uv.y > 0.82 ≈ y 558); shorter row_height keeps all 7 rows in the opaque zone.
list{ collection = "daily", x = 24, y = 312, w = W - 48, h = 238, row_height = 34,
      template = {
        { bind = "day",   x = 8,        y = 8, size = 15, color = { r = 240, g = 244, b = 252 } },
        { bind = "glyph", x = 120,      y = 6, size = 17, color = { r = 245, g = 240, b = 220 } },
        { bind = "hi",    right = 70,   y = 8, size = 15, color = { r = 245, g = 247, b = 252 } },
        { bind = "lo",    right = 10,   y = 8, size = 15, halign = "right", color = { r = 190, g = 198, b = 214 } },
      } }
