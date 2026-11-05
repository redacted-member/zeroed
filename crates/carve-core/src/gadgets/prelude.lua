-- Zeroed gadget library.
--
-- Everything here is built on carve.call(command, args), which runs one
-- kernel command (see the kernel API document) and answers its result as a
-- table. Lengths are millimetres, angles degrees. carve.message(text) adds a
-- line to the messages the form shows after the run; print does the same.

local function call(command, args)
  return carve.call(command, args or {})
end

function print(...)
  local parts = {}
  for i = 1, select("#", ...) do
    parts[#parts + 1] = tostring((select(i, ...)))
  end
  carve.message(table.concat(parts, "\t"))
end

-- Job -------------------------------------------------------------------

-- The job setup plus the material rectangle in job coordinates:
-- minX, minY, maxX, maxY, centreX, centreY.
function carve.job()
  local job = call("job.get")
  local w, h = job.width, job.height
  local ox, oy = 0, 0
  local origin = job.xyOrigin
  if origin == "centre" then
    ox, oy = w / 2, h / 2
  elseif origin == "bottomRight" then
    ox = w
  elseif origin == "topLeft" then
    oy = h
  elseif origin == "topRight" then
    ox, oy = w, h
  end
  ox = ox + (job.offsetX or 0)
  oy = oy + (job.offsetY or 0)
  job.minX, job.minY = -ox, -oy
  job.maxX, job.maxY = w - ox, h - oy
  job.centreX, job.centreY = w / 2 - ox, h / 2 - oy
  return job
end

-- "mm" or "inch", the unit the user works in.
function carve.units()
  return call("job.get").units
end

-- Converts a value in the job's units to millimetres.
function carve.mm(value)
  if carve.units() == "inch" then
    return value * 25.4
  end
  return value
end

-- Layers ----------------------------------------------------------------

function carve.layers()
  return call("layers.list")
end

-- The id of the layer called name, created (with colour "#rrggbb" when
-- given) if there is none. It does not become the active layer.
function carve.layer(name, colour)
  for _, layer in ipairs(carve.layers()) do
    if layer.name == name then
      return layer.id
    end
  end
  return call("layers.add", { name = name, colour = colour }).id
end

function carve.active_layer()
  for _, layer in ipairs(carve.layers()) do
    if layer.active then
      return layer.id
    end
  end
  return nil
end

function carve.set_active_layer(id)
  call("layers.setActive", { id = id })
end

-- Vectors ---------------------------------------------------------------

local function point(x, y)
  return { x = x, y = y }
end

-- A contour builder:
--   local c = carve.contour(0, 0):line_to(10, 0):arc_to(10, 10, 1):close()
--   local id = c:add({ layer = id })
-- arc_to takes a bulge: tan(sweep / 4), positive for anticlockwise.
local Contour = {}
Contour.__index = Contour

function carve.contour(x, y)
  local contour = setmetatable({ subpaths = {} }, Contour)
  return contour:move_to(x, y)
end

function Contour:move_to(x, y)
  self.current = { start = point(x, y), segments = carve.array({}), closed = false }
  self.subpaths[#self.subpaths + 1] = self.current
  return self
end

function Contour:line_to(x, y)
  local segments = self.current.segments
  segments[#segments + 1] = { kind = "line", to = point(x, y) }
  return self
end

function Contour:arc_to(x, y, bulge)
  local segments = self.current.segments
  segments[#segments + 1] = { kind = "arc", to = point(x, y), bulge = bulge }
  return self
end

function Contour:bezier_to(c1x, c1y, c2x, c2y, x, y)
  local segments = self.current.segments
  segments[#segments + 1] = {
    kind = "cubic",
    c1 = point(c1x, c1y),
    c2 = point(c2x, c2y),
    to = point(x, y),
  }
  return self
end

-- Closes the current run; the kernel adds the closing line when needed.
function Contour:close()
  self.current.closed = true
  return self
end

function Contour:path()
  return { subpaths = carve.array(self.subpaths) }
end

-- Adds the contour to the document. options: layer (id), name.
function Contour:add(options)
  return carve.add_path(self:path(), options)
end

-- Adds a path table ({ subpaths = ... } as the kernel API describes it) and
-- answers the new entity id.
function carve.add_path(path, options)
  options = options or {}
  local entity = {
    kind = "path",
    layer = options.layer or 0,
    sheet = 0,
    name = options.name,
    path = path,
  }
  return call("entities.create", { entity = entity }).id
end

-- points: a list of { x, y } pairs or { x = , y = } tables.
function carve.polyline(points, closed, options)
  local function xy(p)
    return p.x or p[1], p.y or p[2]
  end
  local contour = carve.contour(xy(points[1]))
  for i = 2, #points do
    contour:line_to(xy(points[i]))
  end
  if closed then
    contour:close()
  end
  return contour:add(options)
end

function carve.circle(cx, cy, radius, options)
  return carve.contour(cx + radius, cy)
    :arc_to(cx - radius, cy, 1)
    :arc_to(cx + radius, cy, 1)
    :close()
    :add(options)
end

function carve.rectangle(x, y, width, height, options)
  return carve.contour(x, y)
    :line_to(x + width, y)
    :line_to(x + width, y + height)
    :line_to(x, y + height)
    :close()
    :add(options)
end

-- An open arc around (cx, cy) from start_deg to end_deg, anticlockwise
-- when end_deg is larger.
function carve.arc(cx, cy, radius, start_deg, end_deg, options)
  local a0, a1 = math.rad(start_deg), math.rad(end_deg)
  local sweep = a1 - a0
  local pieces = math.max(1, math.ceil(math.abs(sweep) / math.pi))
  local contour = carve.contour(cx + radius * math.cos(a0), cy + radius * math.sin(a0))
  local step = sweep / pieces
  for i = 1, pieces do
    local a = a0 + step * i
    contour:arc_to(cx + radius * math.cos(a), cy + radius * math.sin(a), math.tan(step / 4))
  end
  return contour:add(options)
end

function carve.entity(id)
  return call("entities.get", { id = id })
end

function carve.delete(ids)
  call("entities.delete", { ids = carve.array(ids) })
end

-- Selection -------------------------------------------------------------

function carve.selection()
  return call("selection.get").ids or {}
end

function carve.select(ids)
  call("selection.set", { ids = carve.array(ids) })
end

-- Tools and toolpaths ---------------------------------------------------

function carve.tools()
  return call("tools.list").tools
end

-- The first tool that filter accepts: a function(tool) -> boolean, or a
-- piece of text the tool's name contains.
function carve.find_tool(filter)
  for _, tool in ipairs(carve.tools()) do
    local ok
    if type(filter) == "function" then
      ok = filter(tool)
    else
      ok = string.find(tool.name, filter, 1, true) ~= nil
    end
    if ok then
      return tool
    end
  end
  return nil
end

-- The tool with database id `id` (a `tool` dialog field's value), or nil.
function carve.tool(id)
  for _, tool in ipairs(carve.tools()) do
    if tool.id == id then
      return tool
    end
  end
  return nil
end

-- The widest end mill no wider than max_diameter, or nil.
function carve.end_mill(max_diameter)
  local best
  for _, tool in ipairs(carve.tools()) do
    local d = tool.diameter
    if tool.toolType.kind == "endMill" and d <= max_diameter + 1e-9 then
      if best == nil or d > best.diameter then
        best = tool
      end
    end
  end
  return best
end

-- Calculates a toolpath and answers its id. spec: strategy ("profile",
-- "pocket", "drill", ...), tool (from carve.tools), params, ids, name.
function carve.toolpath(spec)
  local tool = {}
  for k, v in pairs(spec.tool) do
    if k ~= "group" and k ~= "toolTypeName" and k ~= "diameter" then
      tool[k] = v
    end
  end
  local reply = call("cam.compute", {
    strategy = spec.strategy,
    name = spec.name,
    tool = tool,
    params = spec.params or {},
    ids = carve.array(spec.ids),
  })
  for _, warning in ipairs(reply.warnings or {}) do
    if type(warning) == "table" then
      carve.message(tostring(warning.message))
    else
      carve.message(tostring(warning))
    end
  end
  return reply.id
end
