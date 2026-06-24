frame{ asset = "window.png", x = 0, y = 0, w = 480, h = 320, slice = { left = 16, right = 16, top = 34, bottom = 16 }, center = "hollow", anchor = { "left", "right", "top", "bottom" } }
fill{ path = circle{ cx = 26, cy = 11, r = 3.5 }, color = { r = 80, g = 240, b = 110 }, anchor = { "left", "top" } }
fill{ path = circle{ cx = 38, cy = 11, r = 3.5 }, color = { r = 255, g = 190, b = 40 }, anchor = { "left", "top" } }
fill{ path = circle{ cx = 50, cy = 11, r = 3.5 }, color = { r = 255, g = 70, b = 60 }, anchor = { "left", "top" } }
text{ text = "carapace://files", font = "vt323.ttf", size = 16, x = 66, y = 8, color = { r = 150, g = 235, b = 180 }, anchor = { "left", "top" } }
fill{ path = rect{ x = 392, y = 9, w = 26, h = 2 }, color = { r = 26, g = 30, b = 38 }, anchor = { "right", "top" } }
fill{ path = rect{ x = 392, y = 13, w = 26, h = 2 }, color = { r = 26, g = 30, b = 38 }, anchor = { "right", "top" } }
fill{ path = rect{ x = 392, y = 17, w = 26, h = 2 }, color = { r = 26, g = 30, b = 38 }, anchor = { "right", "top" } }
fill{ path = rect{ x = 392, y = 21, w = 26, h = 2 }, color = { r = 26, g = 30, b = 38 }, anchor = { "right", "top" } }
fill{ path = rounded_rect{ x = 424, y = 8, w = 18, h = 18, radius = 3 }, color = { r = 38, g = 44, b = 54 }, anchor = { "right", "top" } }
fill{ path = rounded_rect{ x = 446, y = 8, w = 18, h = 18, radius = 3 }, color = { r = 58, g = 30, b = 32 }, anchor = { "right", "top" } }
text{ text = "_", font = "vt323.ttf", size = 17, x = 429, y = 6, color = { r = 210, g = 222, b = 235 }, anchor = { "right", "top" } }
text{ text = "X", font = "vt323.ttf", size = 16, x = 451, y = 9, color = { r = 255, g = 170, b = 165 }, anchor = { "right", "top" } }
region{ path = rect{ x = 0, y = 0, w = 480, h = 32 }, anchor = { "left", "right", "top" }, on_press = function() host.begin_drag() end }
region{ path = rect{ x = 446, y = 8, w = 18, h = 18 }, anchor = { "right", "top" }, on_press = function() host.close() end }
region{ path = rect{ x = 424, y = 8, w = 18, h = 18 }, anchor = { "right", "top" }, on_press = function() host.minimize() end }
view{ id = "app", x = 16, y = 34, w = 448, h = 270, anchor = { "left", "right", "top", "bottom" }, min = { w = 288, h = 150 } }
