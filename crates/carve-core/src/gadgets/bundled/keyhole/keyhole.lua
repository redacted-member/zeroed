-- Keyhole Toolpath: hanging slots cut into the back of a sign with a keyhole
-- cutter. The center of each selected vector is an entry point: the tool
-- plunges there to the slot depth, runs along the slot, comes back and lifts
-- out where it went in. Optional preview vectors show the slot outline on
-- the surface.

local selection_message =
  "Select one or more circular vectors. Their centers are the entry points of the keyhole slots."

gadget = {
  title = "Keyhole Toolpath",
  description = "Creates keyhole slot toolpaths for hanging a sign: the tool plunges at the entry point, runs along the slot at the slot depth, comes back to the entry point and retracts. Select one or more vectors first; the center of each is the entry point of a slot.",
  selection = selection_message,
  dialog = {
    fields = {
      { id = "direction", label = "Slot orientation", kind = "choice", default = "bottomToTop",
        group = "Slot Orientation",
        options = {
          { value = "bottomToTop", label = "Vertical, bottom to top" },
          { value = "topToBottom", label = "Vertical, top to bottom" },
          { value = "leftToRight", label = "Horizontal, left to right" },
          { value = "rightToLeft", label = "Horizontal, right to left" },
        } },
      { id = "depth", label = "Slot depth", kind = "length", default = 6.35, min = 0.01,
        group = "Slot Dimensions",
        tooltip = "Depth below the surface of the wide part of the cutter." },
      { id = "length", label = "Slot length (L)", kind = "length", default = 25.4, min = 0.01,
        group = "Slot Dimensions",
        tooltip = "How far the tool travels from the entry point along the slot." },
      { id = "preview", label = "Create preview vectors of the outline on the surface", kind = "check",
        default = true, group = "Preview Drawing" },
      { id = "hole", label = "Entry hole diameter", kind = "length", default = 12.7, min = 0.01,
        group = "Preview Drawing", enabledBy = "preview",
        tooltip = "The hole the head of the keyhole cutter makes at the entry point." },
      { id = "slot", label = "Slot diameter", kind = "length", default = 6.35, min = 0.01,
        group = "Preview Drawing", enabledBy = "preview",
        tooltip = "Width of the slot the neck of the cutter leaves on the surface." },
      { id = "layer", label = "Preview layer name", kind = "text", default = "Keyhole Preview",
        group = "Preview Drawing", enabledBy = "preview" },
      { id = "tool", label = "Tool", kind = "tool", group = "Toolpath",
        tooltip = "Set up an end mill to stand in for the keyhole cutter; its feeds and speeds are used." },
      { id = "name", label = "Toolpath name", kind = "text", default = "Keyhole Toolpath",
        group = "Toolpath" },
    },
  },
}

-- Unit step along the slot for each orientation.
local steps = {
  bottomToTop = { 0, 1 },
  topToBottom = { 0, -1 },
  leftToRight = { 1, 0 },
  rightToLeft = { -1, 0 },
}

-- Draws the outline of one slot: the entry hole of radius r at (cx, cy) and
-- the slot of half width h running length l along (dx, dy) with a round end.
local function draw_slot(cx, cy, dx, dy, r, h, l, layer)
  -- Local frame: u along the slot, v across it to the left.
  local function at(u, v)
    return cx + dx * u - dy * v, cy + dy * u + dx * v
  end
  local a = h < r and math.sqrt(r * r - h * h) or 0
  if h < r and l > a then
    -- One outline: out along one side, round the end, back along the other
    -- side, then the long way round the entry hole.
    local theta = math.atan(a, h)
    local sweep = math.pi + 2 * theta
    local x0, y0 = at(a, -h)
    local x1, y1 = at(l, -h)
    local x2, y2 = at(l, h)
    local x3, y3 = at(a, h)
    return {
      carve.contour(x0, y0)
        :line_to(x1, y1)
        :arc_to(x2, y2, 1)
        :line_to(x3, y3)
        :arc_to(x0, y0, math.tan(sweep / 4))
        :close()
        :add({ layer = layer, name = "Keyhole" }),
    }
  end
  -- The slot does not leave the hole, or is as wide as it: draw the two
  -- shapes on their own.
  local ids = { carve.circle(cx, cy, r, { layer = layer, name = "Keyhole" }) }
  local x0, y0 = at(0, -h)
  local x1, y1 = at(l, -h)
  local x2, y2 = at(l, h)
  local x3, y3 = at(0, h)
  ids[#ids + 1] = carve.contour(x0, y0)
    :line_to(x1, y1)
    :arc_to(x2, y2, 1)
    :line_to(x3, y3)
    :arc_to(x0, y0, 1)
    :close()
    :add({ layer = layer, name = "Keyhole" })
  return ids
end

function run(values)
  local ids = carve.selection()
  if #ids == 0 then
    error(selection_message)
  end
  local tool = carve.tool(values.tool)
  if tool == nil then
    error("choose a tool: set up an end mill for the keyhole cutter's feeds and speeds")
  end
  local name = values.name
  if name == "" then
    name = "Keyhole Toolpath"
  end
  carve.toolpath({
    strategy = "keyhole",
    name = name,
    tool = tool,
    params = { direction = values.direction, depth = values.depth, length = values.length },
    ids = ids,
  })

  if values.preview then
    local layer_name = values.layer
    if layer_name == "" then
      layer_name = "Keyhole Preview"
    end
    local layer = carve.layer(layer_name)
    local step = steps[values.direction]
    -- One outline per vector the toolpath cuts a slot for: a group gives
    -- each vector inside it, as the toolpath does.
    local function outline(id)
      local entity = carve.call("entities.summary", { id = id })
      if entity.children then
        for _, child in ipairs(entity.children) do
          outline(child)
        end
        return
      end
      -- A path with nothing in it has no center: the toolpath cuts no slot
      -- for it, so it gets no outline either.
      if entity.kind == "path" then
        local path = carve.call("entities.get", { id = id }).path
        if path == nil or path.subpaths == nil or #path.subpaths == 0 then
          return
        end
      end
      local b = entity.bbox
      draw_slot((b[1] + b[3]) / 2, (b[2] + b[4]) / 2, step[1], step[2],
        values.hole / 2, values.slot / 2, values.length, layer)
    end
    for _, id in ipairs(ids) do
      outline(id)
    end
  end
  carve.select(ids)
  carve.message(string.format("%s calculated with %s.", name, tool.name))
end
