-- Create Rounding Toolpath: for rotary jobs, a toolpath that turns a rough
-- blank, round or square, down to a finished diameter along the whole job.
-- The tool's stepover and pass depth set the passes, as the database has
-- them.

gadget = {
  title = "Create Rounding Toolpath",
  description = "Turns a round or square blank on the rotary axis down to a finished diameter.",
  dialog = {
    fields = {
      { id = "blank", label = "Blank shape", kind = "choice", default = "round",
        options = {
          { value = "round", label = "Round" },
          { value = "square", label = "Square" },
        } },
      { id = "size", label = "Blank diameter or width", kind = "length", default = 0, min = 0,
        tooltip = "Diameter of round stock or side of square stock; 0 uses the job's rotary blank." },
      { id = "finish", label = "Finished diameter", kind = "length", default = 40, min = 0.1 },
      { id = "allowance", label = "Allowance", kind = "length", default = 0, min = 0,
        tooltip = "Material left on the finished diameter for a later pass." },
      { id = "method", label = "Machining method", kind = "choice", default = "radial",
        options = {
          { value = "radial", label = "Radial (around the cylinder)" },
          { value = "raster", label = "Raster (along the cylinder)" },
          { value = "optimizedRaster", label = "Optimized raster (square blank only)" },
        } },
      { id = "tool", label = "Tool", kind = "tool",
        tooltip = "Its stepover, pass depth and feeds are used as the database has them." },
      { id = "name", label = "Toolpath name", kind = "text", default = "Rounding" },
    },
  },
}

function run(values)
  local job = carve.job()
  if job.kind ~= "rotary" then
    error("set the job up as a rotary job first (Job Setup)")
  end
  local tool = carve.tool(values.tool)
  if tool == nil then
    error("choose a tool from the tool database")
  end
  local name = values.name
  if name == "" then
    name = "Rounding"
  end
  carve.toolpath({
    strategy = "rounding",
    name = name,
    tool = tool,
    params = {
      blank = values.blank,
      blankSize = values.size,
      finishDiameter = values.finish,
      allowance = values.allowance,
      method = values.method,
    },
    ids = {},
  })
  carve.message(string.format("%s calculated with %s.", name, tool.name))
end
