-- The dormant frame, as a byte-count control. A frame that is not armed
-- must cost nothing that grows with the number of frames: no allocation,
-- no clock read, no directory listing, and one `lfs.attributes` every
-- `PROBE_EVERY` frames and no other call into the filesystem.
--
-- What is proved. With the collector stopped, a hundred thousand dormant
-- frames leave `collectgarbage('count')` exactly where it was. Across
-- those frames the stubbed `lfs.attributes` is called exactly 12,500
-- times — one frame in eight, the arm file and nothing else — while
-- `lfs.dir` and `os.clock` are never called at all, so the dormant frame
-- neither lists nor reads a clock. Eight frames from a fresh load show
-- the probe on the eighth and on no other, so the probe is one call on a
-- probing frame rather than a burst. A probe that answers leaves the
-- executor armed and the frame after it lists again, so the path is
-- escapable: a dormant executor that cannot see the arm file is a dead
-- one. The export host does the same, because both hosts drive the one
-- frame.
--
-- The measurement is taken after a warm-up frame, and the reading of
-- `collectgarbage('count')` that it is compared against is taken after
-- that frame, not before it. Measured on the interpreter this harness
-- pins, the callback wrapper the executor registers grows the count by
-- 0.797 KB over its first calls — `pcall` reserving stack once — and by
-- zero per call after. Without the warm-up this control would redden on a
-- path that costs nothing per frame, which is the opposite of what it is
-- for.
--
-- The stub matters as much as the count. The model's `lfs.attributes`
-- spawns `cmd.exe`, and 12,500 of those would make this suite unrunnable,
-- so the wrapper delegates to the model while the executor loads and is
-- flipped to a counting stub that touches no disk before the measurement
-- begins.
--
-- The mutations this suite exists to catch. Build a closure per call in
-- the callback wrapper and the byte count parts from where it started,
-- while every call count stays where it was: the control separates
-- allocation from behaviour. Drop the `PROBE_EVERY` guard and the dormant
-- frame probes every frame, which reddens the call count at 100,000
-- against 12,500 while the byte count stays green.
--
-- The wake's latency, the ordered disarm and the events lines are not
-- this suite's, and it says nothing about them.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The figure the executor publishes, the suite's own copy.
local PROBE_EVERY = 8

-- The entries of a directory as the model lists them, sorted, dots dropped.
-- It takes the model's own `lfs.dir`, kept aside at the load, rather than
-- the counting wrapper: a listing the suite makes is not one the executor
-- made.
local function entries(spy, dir)
  local names = {}
  for name in spy.rawdir(dir) do
    if name ~= "." and name ~= ".." then
      names[#names + 1] = name
    end
  end
  table.sort(names)
  return table.concat(names, " ")
end

-- A loaded state over a fresh sandbox with a counter on each of the three
-- calls a dormant frame must not make often or at all. The wrappers are
-- installed before the load, because the executor reads `lfs.attributes`
-- once at load and keeps it, and they delegate to the model so the load's
-- own rotation and sweep still work. After the load the counters are
-- zeroed and the attributes wrapper is left in `answer` mode: it counts
-- and returns what `answer` says, without going near a disk.
--
-- Returns the namespace, the state, the host, and a table carrying the
-- three counts and the mode.
local function loaded(state)
  local host = {}
  local box = t.sandbox()
  host.writedir = box .. SAVED
  host.tempdir = box .. TEMP
  host.clock = 0
  local env = t.state(state, host)
  local spy = { attributes = 0, dir = 0, clock = 0, answer = nil, model = false }
  local attributes, dir, clock = env.lfs.attributes, env.lfs.dir, env.os.clock
  spy.rawdir = dir
  env.lfs.attributes = function(path, mode)
    spy.attributes = spy.attributes + 1
    if spy.model then
      return attributes(path, mode)
    end
    return spy.answer
  end
  env.lfs.dir = function(path)
    spy.dir = spy.dir + 1
    return dir(path)
  end
  env.os.clock = function()
    spy.clock = spy.clock + 1
    return clock()
  end
  spy.model = true
  t.load_executor(env)()
  spy.model = false
  spy.attributes, spy.dir, spy.clock = 0, 0, 0
  local E = rawget(env, NAME)
  t.eq(type(E), "table", state .. ": the namespace is published")
  return E, env, host, spy
end

-- The frame callback of a loaded host, whichever host it is.
local function framer(state, env, host)
  if state == "export" then
    return rawget(env, "LuaExportAfterNextFrame")
  end
  return host.callbacks.onSimulationFrame
end

--------------------------------------------------------------------------------
-- A hundred thousand dormant frames
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("hook")
  local frame = framer("hook", env, host)
  E.armed = false
  -- The warm-up frame, whose cost is the wrapper's one-off reservation and
  -- not a per-frame cost. Everything the measurement compares is read
  -- after it.
  frame()
  spy.attributes, spy.dir, spy.clock = 0, 0, 0
  collectgarbage()
  collectgarbage("stop")
  local before = collectgarbage("count")
  for _ = 1, 100000 do
    frame()
  end
  local after = collectgarbage("count")
  collectgarbage("restart")
  t.eq(after, before, "the hundred thousand dormant frames allocate nothing")
  t.eq(spy.attributes, 12500, "one lfs.attributes in eight and no more")
  t.eq(spy.dir, 0, "the dormant frame never lists")
  t.eq(spy.clock, 0, "the dormant frame never reads the clock")
  -- The warm-up frame is inside this figure.
  t.eq(E.tick, 100001, "every frame advanced the counter")
  t.eq(E.armed, false, "and none of them found an arm file")
end

--------------------------------------------------------------------------------
-- The probe is on the eighth frame and on no other
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("hook")
  local frame = framer("hook", env, host)
  E.armed = false
  for i = 1, PROBE_EVERY do
    frame()
    t.eq(spy.attributes, i == PROBE_EVERY and 1 or 0,
      "frame " .. i .. ": the probe has fired " .. (i == PROBE_EVERY and "once" or "not at all"))
    t.eq(spy.dir, 0, "frame " .. i .. ": and nothing listed")
  end
  t.eq(E.tick, PROBE_EVERY, "eight frames, eight ticks")
  t.eq(E.armed, false, "a probe that answers nothing leaves it dormant")
end

--------------------------------------------------------------------------------
-- A probe that answers arms the executor, and the frame after it lists
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("hook")
  local frame = framer("hook", env, host)
  E.armed = false
  spy.answer = "file"
  for _ = 1, PROBE_EVERY do
    frame()
  end
  t.eq(spy.attributes, 1, "the probing frame made the one call")
  t.eq(E.armed, true, "which found the arm file")
  t.eq(spy.dir, 0, "the probing frame itself listed nothing")
  t.eq(entries(spy, E.req), "", "there is nothing to take")
  frame()
  t.eq(spy.dir, 1, "the frame after it listed the request directory")
  t.eq(spy.clock, 1, "and read the clock the budget is spent from")
  t.eq(E.tick, PROBE_EVERY + 1, "nine frames, nine ticks")
end

--------------------------------------------------------------------------------
-- The export host runs the same frame
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("export")
  local frame = framer("export", env, host)
  E.armed = false
  local frames = PROBE_EVERY * 40
  for _ = 1, frames do
    frame()
  end
  t.eq(E.tick, frames, "export: every frame advanced the counter")
  t.eq(spy.attributes, frames / PROBE_EVERY, "export: one probe in eight")
  t.eq(spy.dir, 0, "export: and nothing listed")
  t.eq(spy.clock, 0, "export: nor any clock read")
  t.eq(E.armed, false, "export: still dormant")
end
