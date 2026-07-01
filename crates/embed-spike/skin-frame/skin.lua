-- MEDIEVAL DOOR FRAME — the skin is CHROME that wraps a transparent opening.
-- Everything inside the arch + jambs is left UNDRAWN, so it renders transparent and the host
-- (Flutter) content shows through the doorway. Light falls from the top-left (a wall torch).
-- The arch is built the real way: wedge-shaped voussoir stones fanning around a keystone.
--
-- NOTE: the skin Lua sandbox exposes only the vocab globals — there is no `math` table — so the
-- arch geometry uses a precomputed cos/sin table over fixed 20-degree steps (9 voussoirs).

local W, H      = 360, 640
local JAMB      = 90     -- opening spans x = JAMB .. W-JAMB
local SPRING    = 340    -- springline: the arch springs from here
local RI, RO    = 90, 150 -- arch inner / outer radius
local CX        = 180    -- arch centre x
local SILL      = 565    -- threshold top

-- palette: torch-lit stone, warm not cold ------------------------------------
local MORTAR    = { r = 20, g = 18, b = 14 }
local STONE_D   = { r = 58, g = 54, b = 46 }
local STONE_M   = { r = 92, g = 86, b = 74 }
local STONE_L   = { r = 128, g = 120, b = 104 }
local IRON_D    = { r = 28, g = 26, b = 23 }
local IRON_L    = { r = 82, g = 78, b = 70 }
local TORCH_HOT = { r = 255, g = 214, b = 130 }
local REVEAL    = { r = 150, g = 120, b = 78 } -- warm lit inner bevel
local SHADE     = { r = 34, g = 31, b = 26 } -- shadow-side bevel
local CARVE     = { r = 40, g = 37, b = 31 }
local EMBER     = { r = 60, g = 44, b = 24 }

-- arch sampling: 10 points at 0,20,..,180 deg (no `math` in the sandbox) ------
local COS       = { 1.0, 0.9397, 0.7660, 0.5, 0.1736, -0.1736, -0.5, -0.7660, -0.9397, -1.0 }
local SIN       = { 0.0, 0.3420, 0.6428, 0.8660, 0.9848, 0.9848, 0.8660, 0.6428, 0.3420, 0.0 }
local function pt(r, i) return { x = CX + r * COS[i], y = SPRING - r * SIN[i] } end -- i = 1..10

-- one ashlar block: lit (top-left) -> shadow (bottom-right) gradient ----------
local function block(x, y, w, h, light, dark)
    fill { path = rect { x = x, y = y, w = w, h = h }, gradient = {
        type = "linear", from = { x = x, y = y }, to = { x = x + w, y = y + h },
        stops = { { at = 0, color = light }, { at = 1, color = dark } } } }
end

local function stud(cx, cy, r) -- iron bolt: dark disc + a top-left glint
    fill { path = rounded_rect { x = cx - r, y = cy - r, w = 2 * r, h = 2 * r, radius = r }, color = IRON_D }
    local g = r * 0.42
    fill { path = rounded_rect { x = cx - r * 0.5, y = cy - r * 0.5, w = g * 2, h = g * 2, radius = g }, color = IRON_L }
end

-- 1. mortar backing behind every stone region (so gaps read as mortar lines) --
fill { path = rect { x = 0, y = 0, w = JAMB, h = SILL }, color = MORTAR }
fill { path = rect { x = W - JAMB, y = 0, w = JAMB, h = SILL }, color = MORTAR }
fill { path = rect { x = JAMB, y = 0, w = W - 2 * JAMB, h = SPRING - RI }, color = MORTAR }
fill { path = rect { x = 0, y = SILL, w = W, h = H - SILL }, color = MORTAR }
do -- arch mortar annulus
    local ann = {}
    for i = 1, 10 do ann[#ann + 1] = pt(RO, i) end
    for i = 10, 1, -1 do ann[#ann + 1] = pt(RI, i) end
    fill { path = ann, color = MORTAR }
end

-- 2. jamb courses (running bond: alternate a full block / a split pair) -------
local course, cy = 0, 0
while cy < SILL - 8 do
    local rem = SILL - cy
    local h = (rem < 76 and rem or 76) - 4
    if course % 2 == 0 then
        block(3, cy + 3, JAMB - 6, h, STONE_M, STONE_D)
        block(W - JAMB + 3, cy + 3, JAMB - 6, h, STONE_D, STONE_M)
    else
        block(3, cy + 3, (JAMB - 6) / 2 - 2, h, STONE_L, STONE_M)
        block(3 + (JAMB - 6) / 2 + 2, cy + 3, (JAMB - 6) / 2 - 2, h, STONE_M, STONE_D)
        block(W - JAMB + 3, cy + 3, JAMB - 6, h, STONE_D, STONE_M)
    end
    course, cy = course + 1, cy + 76
end

-- 3. lintel band above the arch, with a carved inscription -------------------
block(JAMB + 3, 3, W - 2 * JAMB - 6, SPRING - RI - 8, STONE_M, STONE_D)
text { text = "· CARAPACE ·", x = JAMB + 30, y = 30, size = 22, color = CARVE }

-- 4. voussoirs + keystone (one wedge per 20-degree segment) ------------------
for k = 1, 9 do
    local mid = (k + 0.5) -- lit stones face up-left
    local light = STONE_M
    if mid >= 6 then light = STONE_L elseif mid < 3.5 then light = STONE_D end
    local p = { pt(RI + 2, k), pt(RI + 2, k + 1), pt(RO - 2, k + 1), pt(RO - 2, k) }
    fill { path = p, gradient = {
        type = "linear", from = pt(RI + 2, k), to = pt(RO - 2, k + 1),
        stops = { { at = 0, color = STONE_D }, { at = 1, color = light } } } }
end
do -- keystone: the central wedge (segment 5, 80..100 deg), protruding + brighter
    local p = { pt(RI + 2, 5), pt(RI + 2, 6), pt(RO + 12, 6), pt(RO + 12, 5) }
    fill { path = p, gradient = {
        type = "linear", from = pt(RI, 5), to = pt(RO + 12, 6),
        stops = { { at = 0, color = STONE_M }, { at = 1, color = STONE_L } } } }
    stud(CX, SPRING - (RI + RO) / 2, 5)
end

-- 5. lit inner reveal (top-left warm, lower-right shadow) — frames the opening -
do
    local rl = {} -- rim, lit left+top half (i = 5..10)
    for i = 5, 10 do rl[#rl + 1] = pt(RI, i) end
    for i = 10, 5, -1 do rl[#rl + 1] = pt(RI + 7, i) end
    fill { path = rl, color = REVEAL }
    local rs = {} -- rim, shaded right half (i = 1..5)
    for i = 1, 5 do rs[#rs + 1] = pt(RI, i) end
    for i = 5, 1, -1 do rs[#rs + 1] = pt(RI + 7, i) end
    fill { path = rs, color = SHADE }
end
fill { path = rect { x = JAMB - 7, y = SPRING, w = 7, h = SILL - SPRING }, color = REVEAL } -- left jamb reveal
fill { path = rect { x = W - JAMB, y = SPRING, w = 7, h = SILL - SPRING }, color = SHADE } -- right jamb reveal

-- 6. iron studs across the frame ---------------------------------------------
stud(45, 150, 6); stud(45, 470, 6)
stud(W - 45, 150, 6); stud(W - 45, 470, 6)

-- 7. threshold: two worn steps -----------------------------------------------
block(0, SILL + 3, W, 34, STONE_M, STONE_D)
block(24, SILL + 40, W - 48, H - SILL - 44, STONE_L, STONE_M)

-- 8. wall torch on the left jamb — TAP to light it (round-trips to host.toggle)
do
    local tx, ty = 45, 420
    fill { path = { { x = tx - 3, y = ty }, { x = tx + 3, y = ty }, { x = tx + 9, y = ty + 34 }, { x = tx - 9, y = ty + 34 } }, color = IRON_D }
    fill { path = rounded_rect { x = tx - 14, y = ty - 8, w = 28, h = 16, radius = 6 }, color = IRON_L }
    local flame = { { x = tx, y = ty - 46 }, { x = tx + 13, y = ty - 6 }, { x = tx, y = ty - 2 }, { x = tx - 13, y = ty - 6 } }
    fill { path = flame, color = EMBER }                       -- unlit ember
    value_fill { path = flame, value = "lit", color = TORCH_HOT } -- lit flame (host "lit")
    region { path = rect { x = tx - 20, y = ty - 52, w = 40, h = 96 }, on_press = function() host.toggle() end }
end
