-- Gear Maker: draws a spur gear outline with involute tooth flanks and an
-- optional bore, on a layer of its own.

gadget = {
  title = "Gear Maker",
  description = "Draws a spur gear with involute teeth from the tooth count and module.",
  dialog = {
    fields = {
      { id = "teeth", label = "Number of teeth", kind = "number", default = 20, min = 8, max = 300,
        tooltip = "How many teeth the gear has." },
      { id = "module", label = "Module", kind = "length", default = 2, min = 0.1,
        tooltip = "Pitch diameter divided by the number of teeth." },
      { id = "pressure", label = "Pressure angle", kind = "choice", default = "20",
        options = {
          { value = "14.5", label = "14.5 degrees" },
          { value = "20", label = "20 degrees" },
          { value = "25", label = "25 degrees" },
        } },
      { id = "bore", label = "Bore diameter", kind = "length", default = 6, min = 0,
        tooltip = "Diameter of the center hole. Zero leaves it out." },
      { id = "position", label = "Center", kind = "choice", default = "job",
        options = {
          { value = "job", label = "Center of the job" },
          { value = "origin", label = "Job origin" },
        } },
      { id = "layer", label = "Layer", kind = "text", default = "Gear" },
    },
  },
}

-- Involute function: tan(a) - a.
local function inv(a)
  return math.tan(a) - a
end

local function polar(cx, cy, r, a)
  return cx + r * math.cos(a), cy + r * math.sin(a)
end

function run(values)
  local teeth = math.floor(values.teeth + 0.5)
  local m = values.module
  local alpha = math.rad(tonumber(values.pressure))

  local rp = m * teeth / 2          -- pitch radius
  local rb = rp * math.cos(alpha)   -- base radius
  local ra = rp + m                 -- addendum (tip) radius
  local rf = rp - 1.25 * m          -- dedendum (root) radius
  if rf <= 0 then
    error("the gear needs more teeth for this module")
  end
  if values.bore >= 2 * rf then
    error("the bore is wider than the root of the teeth")
  end

  local cx, cy = 0, 0
  if values.position == "job" then
    local job = carve.job()
    cx, cy = job.centreX, job.centreY
  end

  -- Half the angular width of a tooth at radius r (r >= rb).
  local half_pitch = math.pi / (2 * teeth)
  local function half_width(r)
    return half_pitch + inv(alpha) - inv(math.acos(math.min(1, rb / r)))
  end

  local flank_start = math.max(rb, rf)
  local steps = 8
  local tip_half = half_width(ra)
  local root_half = half_width(flank_start)
  if tip_half <= 0 then
    error("the teeth come to a point; use more teeth or a smaller pressure angle")
  end

  local pitch_angle = 2 * math.pi / teeth
  local root_sweep = pitch_angle - 2 * root_half
  if root_sweep <= 0 then
    error("the teeth overlap at the root")
  end

  local contour
  for i = 0, teeth - 1 do
    local c = pitch_angle * i
    -- Rising flank, from the root circle to the tip.
    local x, y = polar(cx, cy, rf, c - root_half)
    if contour == nil then
      contour = carve.contour(x, y)
    end
    if flank_start > rf then
      contour:line_to(polar(cx, cy, flank_start, c - root_half))
    end
    for s = 1, steps do
      local r = flank_start + (ra - flank_start) * s / steps
      contour:line_to(polar(cx, cy, r, c - half_width(r)))
    end
    -- Tip land.
    local tx, ty = polar(cx, cy, ra, c + tip_half)
    contour:arc_to(tx, ty, math.tan(2 * tip_half / 4))
    -- Falling flank, back down to the root circle.
    for s = steps - 1, 0, -1 do
      local r = flank_start + (ra - flank_start) * s / steps
      contour:line_to(polar(cx, cy, r, c + half_width(r)))
    end
    if flank_start > rf then
      contour:line_to(polar(cx, cy, rf, c + root_half))
    end
    -- Root land to the next tooth.
    local nx, ny = polar(cx, cy, rf, c + pitch_angle - root_half)
    contour:arc_to(nx, ny, math.tan(root_sweep / 4))
  end
  contour:close()

  local layer = carve.layer(values.layer ~= "" and values.layer or "Gear")
  local ids = { contour:add({ layer = layer, name = "Gear" }) }
  if values.bore > 0 then
    ids[#ids + 1] = carve.circle(cx, cy, values.bore / 2, { layer = layer, name = "Bore" })
  end
  carve.select(ids)

  carve.message(string.format(
    "Gear with %d teeth: pitch diameter %.3f mm, outside diameter %.3f mm.",
    teeth, 2 * rp, 2 * ra))
end
