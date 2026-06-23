-- window border: a hollow-center 9-slice frame anchored to all four edges
frame{ asset = "window.png", x = 0, y = 0, w = 480, h = 320,
       slice = { left = 16, right = 16, top = 36, bottom = 16 }, center = "hollow",
       anchor = { "left", "right", "top", "bottom" } }
-- title bar fill: full width, fixed height, pinned to the top
fill{ path = rect{ x = 0, y = 0, w = 480, h = 30 }, color = { r = 28, g = 34, b = 46 },
      anchor = { "left", "right", "top" } }
text{ text = "carapace://files", font = "vt323.ttf", size = 18, x = 12, y = 4,
      color = { r = 200, g = 220, b = 255 }, anchor = { "left", "top" } }
-- close / minimize hotspots, pinned to the top-right
region{ path = rect{ x = 456, y = 8, w = 14, h = 14 }, anchor = { "right", "top" },
        on_press = function() host.close() end }
region{ path = rect{ x = 436, y = 8, w = 14, h = 14 }, anchor = { "right", "top" },
        on_press = function() host.minimize() end }
-- whole-window drag region (behind the controls)
region{ path = rect{ x = 0, y = 0, w = 480, h = 30 }, anchor = { "left", "right", "top" },
        on_press = function() host.begin_drag() end }
-- the hosted app's content region: stretches to fill, never smaller than 280x150
view{ id = "app", x = 12, y = 36, w = 456, h = 272,
      anchor = { "left", "right", "top", "bottom" }, min = { w = 280, h = 150 } }
