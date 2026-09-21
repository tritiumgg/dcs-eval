-- The chunk `dcs-mcp live dormant` sends, run against a loaded executor in
-- the modelled hook state. What it proves: the chunk compiles under the
-- pinned 5.1.5 and runs with only the names the state models; the stat it
-- times as the dormant frame's is of a path that is not there, as a
-- dormant frame's is, and not of the arm file a live request has just
-- written; the second stat is of the arm file; the listing is of the
-- request directory, which is empty; and it answers five figures.
--
-- What it does not prove is any figure. The model's filesystem shells out
-- to `cmd` and its clock is the suite's, so a number here is the model's
-- and never DCS's: ADR 0025 says what the figures are and only a live run
-- takes them.
--
-- The spies keep the suite fast. The model's `lfs.attributes` and `lfs.dir`
-- each spawn `cmd.exe`, and the chunk calls each in a loop, so each spy asks
-- the model once per path and answers every later call from what it said,
-- the way `executor/dormant.lua` flips its wrapper to a stub. The clock spy
-- moves further than the chunk's whole span on every read, so every loop
-- ends at its first look at the clock, after one batch of calls.
local t = ...

local NAME = "DcsEvalExecutor"
local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- More than the chunk's own span, so one read of the clock ends a loop and
-- floating point cannot land it a hair short.
local STEP = 0.06

local host = {}
local box = t.sandbox()
host.writedir = box .. SAVED
host.tempdir = box .. TEMP
host.clock = 0
local env = t.state("hook", host)
t.load_executor(env)()
local E = rawget(env, NAME)
t.eq(type(E), "table", "the namespace is published")

-- The executor is armed while a request runs, so the arm file is there.
local fh = assert(io.open(E.arm, "wb"))
fh:close()

-- What the model said for each path, the paths in the order first asked
-- about, and how often each spy was called.
local stats, statted, listed = {}, {}, {}
local attributes, dir = env.lfs.attributes, env.lfs.dir
env.lfs.attributes = function(path, mode)
  if stats[path] == nil then
    stats[path] = { answer = attributes(path, mode) }
    statted[#statted + 1] = path
  end
  return stats[path].answer
end
env.lfs.dir = function(path)
  if listed[path] == nil then
    local names = {}
    for name in dir(path) do
      if name ~= "." and name ~= ".." then
        names[#names + 1] = name
      end
    end
    listed[path] = names
  end
  local i = 0
  return function()
    i = i + 1
    return listed[path][i]
  end
end
env.os.clock = function()
  host.clock = host.clock + STEP
  return host.clock
end

local chunk = assert(loadfile(t.root .. "/crates/dcs-mcp/src/live/dormant.lua"))
setfenv(chunk, env)
local out = chunk()
t.eq(type(out), "string", "the chunk answers a string")

local figures, capped = {}, nil
for line in (out .. "\n"):gmatch("([^\n]*)\n") do
  local name, value = line:match("^(%a+_us) (%S+)$")
  if name then
    figures[#figures + 1] = name
    t.check(tonumber(value) ~= nil, name .. " is a number: " .. line)
  else
    capped = line:match("^capped (%S+)$")
    t.check(capped ~= nil, "a line off the grammar: " .. line)
  end
end
t.eq(table.concat(figures, " "), "absent_us present_us list_us pcall_us empty_us",
  "the result has five figures, in order")

t.eq(stats[statted[1]].answer, nil, "the dormant stat is of a path that does not exist")
t.eq(statted[2], E.arm, "the present stat is of the arm file")
t.eq(stats[E.arm].answer, "file", "which is there")
t.check(listed[E.req] ~= nil, "the listing is of the request directory")
t.eq(#listed[E.req], 0, "and it yields nothing")
t.eq(capped, "none", "every loop was ended by the clock, not the cap")
