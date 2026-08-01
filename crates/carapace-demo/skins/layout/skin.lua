local W, H = 480, 320

-- Background, fills the whole window.
fill{ path = rect{ x = 0, y = 0, w = W, h = H },
      color = { r = 18, g = 20, b = 26 },
      anchor = { "left", "right", "top", "bottom" } }

-- Sidebar: 30% of width, capped at 320px, full height.
fill{ path = rect{ x = 0, y = 0, w = 144, h = H },
      color = { r = 44, g = 52, b = 74 },
      anchor = { "left", "top", "bottom" },
      frac = { w = 0.30 }, max = { w = 320 } }
text{ text = "30% (max 320)", x = 16, y = 20, size = 14,
      color = { r = 220, g = 228, b = 245 },
      anchor = { "left", "top" } }

-- Content: occupies the remaining 70% -- BOTH x and w are fractional (x=30%, w=70%).
fill{ path = rect{ x = 144, y = 0, w = W - 144, h = H },
      color = { r = 28, g = 32, b = 42 },
      anchor = { "top", "bottom" },
      frac = { x = 0.30, w = 0.70 } }
text{ text = "content (30%..100%)", x = 160, y = 20, size = 14,
      color = { r = 200, g = 208, b = 226 },
      frac = { x = 0.30 }, anchor = { "top" } }
