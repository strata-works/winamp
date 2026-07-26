local W, H = 400, 680

-- Two-tier text colors: primary = near-white, secondary = softened toward the sky.
local PRI = { r = 252, g = 253, b = 255 }
local SEC = { r = 196, g = 208, b = 230 }
-- Daily "lo" temps: brighter than SEC (they sit on the brightest backgrounds).
local LO  = { r = 228, g = 234, b = 246 }

-- Animated WGSL background (renders UNDER the 2D UI). Host-bound numeric uniforms.
shader{ src = "weather.wgsl", x = 0, y = 0, w = W, h = H,
        uniforms = { condition = "wx_condition", sun = "wx_sun", cond_age = "wx_cond_age",
                     temp = "wx_temp", intensity = "wx_intensity", season = "wx_season" } }

-- Whole-window drag (the skin IS the window). Controls drawn later win hit-testing.
region{ path = rect{ x = 0, y = 0, w = W, h = H }, role = "drag",
        on_press = function() host.begin_drag() end }

-- Bundled Inter (OFL, see LICENSE-Inter.txt): Medium for secondary text, SemiBold for
-- primary. The engine's system-default face renders thin/brittle at small sizes (vello has
-- no CoreText-style stem darkening), so weights are baked into the font files.
local F_PRI = "Inter-SemiBold.ttf"
local F_SEC = "Inter-Medium.ttf"

-- Hero block.
text{ value = "location", font = F_PRI, x = 28, y = 40, size = 26, color = PRI }
text{ value = "condition_text", font = F_SEC, x = 28, y = 76, size = 14, color = SEC }
text{ value = "temp_now", font = F_SEC, x = 28, y = 108, size = 72, color = PRI }
text{ value = "hi_lo", font = F_SEC, x = 30, y = 198, size = 14, color = SEC }
text{ value = "feels", font = F_SEC, x = 160, y = 198, size = 14, color = SEC }

-- Horizontal hourly strip: 12 cells (time above, temp below), evenly spaced.
local hourly_y = 250
local n = 12
local pad = 20
local step = (W - pad * 2) / n
for i = 0, n - 1 do
  local cx = pad + step * i + step / 2
  text{ value = "wx_hour_" .. i .. "_time", font = F_SEC, x = cx, y = hourly_y, size = 11,
        halign = "center", color = SEC }
  text{ value = "wx_hour_" .. i .. "_temp", font = F_PRI, x = cx, y = hourly_y + 20, size = 13,
        halign = "center", color = PRI }
end

-- Vertical daily forecast list (collection = "daily"). Ends above the shader's bottom
-- silhouette band (uv.y > 0.82 ≈ y 558); shorter row_height keeps all 7 rows in the opaque zone.
list{ collection = "daily", x = 24, y = 312, w = W - 48, h = 238, row_height = 34,
      template = {
        { bind = "day",   font = F_PRI, x = 8,      y = 8, size = 15, color = PRI },
        { bind = "glyph", x = 120,      y = 6, size = 17, color = { r = 245, g = 240, b = 220 } },
        { bind = "hi",    font = F_PRI, right = 70, y = 8, size = 15, color = PRI },
        { bind = "lo",    font = F_SEC, right = 10, y = 8, size = 15, halign = "right", color = LO },
      } }

-- ── Location search overlay (M4) ──────────────────────────────────────────────
-- Everything here is gated on the host bool `search_active`: the two value_fill
-- panels draw nothing when it is 0, and every text line binds a host string that
-- is "" while inactive. Declared last, so it composites on top of the scene.
local SEARCH_ROWS = 5
local cardX, cardY = 28, 150
local cardW, cardH = W - 56, 320

-- Full-scene dim, then the card. value_fill draws nothing at value 0, full color at 1.
value_fill{ path = rect{ x = 0, y = 0, w = W, h = H }, value = "search_active",
            color = { r = 6, g = 9, b = 16, a = 150 }, direction = "down" }
value_fill{ path = rounded_rect{ x = cardX, y = cardY, w = cardW, h = cardH, radius = 16 },
            value = "search_active",
            color = { r = 14, g = 18, b = 28, a = 235 }, direction = "down" }

-- Query line.
text{ value = "search_query", font = F_PRI, x = cardX + 22, y = cardY + 20, size = 22, color = PRI }
-- Status line (Type a city… / Searching… / No matches / Offline). "" while results show.
text{ value = "search_status", font = F_SEC, x = cardX + 22, y = cardY + 74, size = 14, color = SEC }
-- Result rows; the selected one carries a "▸" marker baked into the label host-side.
local searchRowY0 = cardY + 70
for i = 0, SEARCH_ROWS - 1 do
  text{ value = "search_row_" .. i .. "_label", font = F_SEC,
        x = cardX + 18, y = searchRowY0 + i * 32, size = 15, color = PRI }
end
-- Navigation hint pinned near the card bottom.
text{ value = "search_hint", font = F_SEC, x = cardX + 22, y = cardY + cardH - 30,
      size = 12, halign = "left", color = SEC }
