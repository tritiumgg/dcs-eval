-- The uncollected-reply sweep: a reply is kept for 300 seconds from the
-- frame it was published on, and removed by the first armed frame to find
-- it that old, the frame that goes back to sleep included. One that ages
-- past it while the executor sleeps stays until the next armed frame,
-- because a sleeping frame does nothing, and that is the budget
-- `executor/dormant` holds. The removals are spent from the tick's budget,
-- one always made, so a backlog is cleared across frames.
--
-- The clock is the suite's: `os.time` delegates to the model while the
-- executor loads and answers `spy.now` afterwards, so 300 seconds cost no
-- real time. The model's `os.clock` stands still unless the suite moves it,
-- which the wrapped `os.remove` does by `spy.cost` milliseconds for each
-- `.res` it removes while a case sets one. Publishing a reply removes its
-- final name before the rename, so a frame that answers a request while
-- `spy.cost` is set spends budget before the sweep runs; the one case that
-- sets it does so only around frames that answer nothing.
--
-- The mutations this suite exists to catch. Never call the sweep and the
-- reply 300 s old is still there. Keep a reply for no time and it is gone
-- the frame it was answered. Drop the budget test and a backlog of three
-- goes in one frame. Test the budget before the first removal too and a
-- frame whose requests spent it removes nothing. Sweep on the sleeping path and a reply past 300 s is
-- gone before anything woke the executor.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The figures the executor holds, the suite's own copies.
local PROBE_EVERY = 8
local QUIET_S = 3
local UNCOLLECTED_S = 300

-- The wall clock this suite hands the executor, advanced by hand.
local NOW = 1000000000

local function write(path, bytes)
  local fh = assert(io.open(path, "wb"))
  fh:write(bytes or "")
  fh:close()
end

-- Whether the reply to `id` is on the disk, through the runner's own `io`.
local function kept(E, id)
  local fh = io.open(E.res .. "\\" .. id .. ".res", "rb")
  if fh then
    fh:close()
    return true
  end
  return false
end

-- The entries of a directory, sorted, dots dropped.
local function entries(env, dir)
  local names = {}
  for name in env.lfs.dir(dir) do
    if name ~= "." and name ~= ".." then
      names[#names + 1] = name
    end
  end
  table.sort(names)
  return table.concat(names, " ")
end

-- Every line of the events log whose first field is `field`.
local function marked(E, field)
  local fh = assert(io.open(E.events, "rb"), "the events log is on the disk")
  local bytes = fh:read("*a")
  fh:close()
  local out = {}
  for line in bytes:gmatch("([^\n]+)") do
    if line:sub(1, #field + 1) == field .. "|" then
      out[#out + 1] = line
    end
  end
  return out
end

-- A loaded state over a fresh sandbox with the suite's clock around it.
-- Returns the namespace, the state, the spy, and the frame callback.
local function loaded(state)
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP, clock = 0 }
  local env = t.state(state, host)
  local spy = { now = NOW, model = true, cost = nil }
  local time, remove = env.os.time, env.os.remove
  env.os.time = function(...)
    if spy.model then
      return time(...)
    end
    return spy.now
  end
  env.os.remove = function(path)
    if spy.cost and path:sub(-4) == ".res" then
      host.clock = host.clock + spy.cost / 1000
    end
    return remove(path)
  end
  t.load_executor(env)()
  spy.model = false
  local E = rawget(env, NAME)
  t.eq(type(E), "table", state .. ": the namespace is published")
  local frame
  if state == "export" then
    frame = rawget(env, "LuaExportAfterNextFrame")
  else
    frame = host.callbacks.onSimulationFrame
  end
  return E, env, spy, frame
end

local function request(E, id)
  write(E.req .. "\\" .. id .. ".req", "op: ping\nfor: " .. E.stamp .. "\n\n")
end

-- Awake off the arm file, the way a client wakes it.
local function armed(E, frame, what)
  write(E.arm)
  for _ = 1, PROBE_EVERY do
    frame()
  end
  t.eq(E.armed, true, what .. ": awake off the arm file")
end

-- Kept at 299 s and removed at 300 s by an executor that never slept: a
-- request every frame keeps the quiet window shut whatever the clock does.
do
  local E, env, spy, frame = loaded("hook")
  armed(E, frame, "armed")
  request(E, "1-a")
  frame()
  t.check(kept(E, "1-a"), "armed: the reply is on the disk the frame it is answered")
  spy.now = NOW + UNCOLLECTED_S - 1
  request(E, "2-a")
  frame()
  t.check(kept(E, "1-a"), "armed: the reply 299 s old is kept")
  spy.now = NOW + UNCOLLECTED_S
  request(E, "3-a")
  frame()
  t.check(not kept(E, "1-a"), "armed: the reply 300 s old is removed")
  t.eq(entries(env, E.res), "2-a.res 3-a.res", "armed: and the two younger replies are kept")
  t.eq(E.armed, true, "armed: by an executor that never slept")
  t.eq(#marked(E, "disarm"), 0, "armed: and never disarmed")
  t.eq(E.raised, 0, "armed: nothing reached the guard")
end

-- The frame that goes back to sleep sweeps like the ones before it.
do
  local E, _, spy, frame = loaded("hook")
  armed(E, frame, "disarm")
  request(E, "1-a")
  frame()
  frame()
  t.eq(E.quiet_since, NOW, "disarm: the window opened")
  spy.now = NOW + UNCOLLECTED_S
  frame()
  t.eq(E.armed, false, "disarm: the quiet period ended it")
  t.check(not kept(E, "1-a"), "disarm: the frame that went back to sleep removed the reply 300 s old")
  t.eq(#marked(E, "disarm"), 1, "disarm: once")
end

-- Nothing while dormant: a reply past the limit outlives a hundred sleeping
-- frames, and the first armed frame after the wake removes it.
do
  local E, _, spy, frame = loaded("hook")
  armed(E, frame, "asleep")
  request(E, "1-a")
  frame()
  frame()
  spy.now = NOW + QUIET_S
  frame()
  t.eq(E.armed, false, "asleep: disarmed with the reply 3 s old")
  t.check(kept(E, "1-a"), "asleep: which the disarm kept")
  spy.now = NOW + UNCOLLECTED_S * 3
  for _ = 1, 100 do
    frame()
  end
  t.eq(E.armed, false, "asleep: nothing woke it")
  t.check(kept(E, "1-a"), "asleep: a reply past 300 s stays until the executor wakes")
  write(E.arm)
  for _ = 1, PROBE_EVERY + 1 do
    frame()
  end
  t.eq(E.armed, true, "asleep: the arm file woke it")
  t.check(not kept(E, "1-a"), "asleep: and an armed frame removed the reply")
end

-- A reply somebody else removed first costs the sweep nothing, and the one
-- behind it still goes.
do
  local E, env, spy, frame = loaded("hook")
  armed(E, frame, "gone")
  request(E, "1-a")
  request(E, "1-b")
  frame()
  assert(os.remove(E.res .. "\\1-a.res"))
  spy.now = NOW + UNCOLLECTED_S
  request(E, "2-a")
  frame()
  t.eq(entries(env, E.res), "2-a.res", "gone: the missing reply cost nothing and the one behind it went")
  t.eq(E.raised, 0, "gone: nothing reached the guard")
end

-- A backlog is spent from the tick budget: each removal costs 5 ms, so the
-- first is made, the second is made at 5 ms, and the third waits at 10 ms
-- for the next frame.
do
  local E, env, spy, frame = loaded("hook")
  armed(E, frame, "backlog")
  request(E, "1-a")
  request(E, "1-b")
  request(E, "1-c")
  frame()
  t.eq(entries(env, E.res), "1-a.res 1-b.res 1-c.res", "backlog: three replies on one frame")
  spy.now = NOW + UNCOLLECTED_S
  spy.cost = 5
  frame()
  t.eq(entries(env, E.res), "1-c.res", "backlog: one reply is left for the next frame")
  frame()
  t.eq(entries(env, E.res), "", "backlog: which the next frame removes")
  t.eq(E.armed, true, "backlog: without sleeping")
  spy.cost = nil
end

-- A frame whose requests spend the whole budget still makes one removal,
-- so under a steady load the ledger shrinks rather than growing: answering
-- the first request costs 10 ms, which leaves the second for the next frame
-- and the sweep nothing, and the reply 300 s old goes all the same.
do
  local E, env, spy, frame = loaded("hook")
  armed(E, frame, "spent")
  request(E, "1-a")
  frame()
  spy.now = NOW + UNCOLLECTED_S
  spy.cost = 10
  request(E, "2-a")
  request(E, "2-b")
  frame()
  spy.cost = nil
  t.eq(entries(env, E.req), "2-b.req", "spent: the budget left the second request for the next frame")
  t.eq(entries(env, E.res), "2-a.res", "spent: one expired reply is removed even with the budget gone")
end

-- The export host sweeps on its own frame.
do
  local E, env, spy, frame = loaded("export")
  armed(E, frame, "export")
  request(E, "1-a")
  frame()
  spy.now = NOW + UNCOLLECTED_S
  request(E, "2-a")
  frame()
  t.eq(entries(env, E.res), "2-a.res", "export: the reply 300 s old is removed on the export host's frame too")
end
