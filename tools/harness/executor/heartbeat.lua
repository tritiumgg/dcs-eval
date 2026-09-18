-- The heartbeat: the file a client reads to decide whether this session is
-- alive and ticking, without asking it anything. `executor/arming` holds the
-- transitions themselves and `executor/dormant` the cost of a sleeping
-- frame; what is here is when the file is written, when it is not, and what
-- it says when it is.
--
-- What is proved. A load writes none, and leaves the output holding what
-- `executor/events` says a load leaves. A dormant executor writes none, over
-- frames spanning several beat intervals of the suite's own clock — that is
-- the check the specification's resolution turns on, because a dormant
-- executor that kept beating would not be dormant. An arm writes one, a
-- disarm writes one, a phase change writes one, a callback repeating the
-- phase it is already in writes none, and an armed session writes one per
-- elapsed interval whether it is idle or answering a request every frame.
-- Every phase change also appends one line to the events log under a first
-- field that is neither of the two dispatch markers.
--
-- The clock is the suite's, not the machine's, wrapped the way
-- `executor/arming` wraps it: it delegates to the model while the executor
-- loads, so the stamp is a real one, and answers `spy.now` afterwards, so no
-- check here waits on a real second.
--
-- The writes are counted through a wrapper on the state's own `io.open`,
-- keyed on the temporary name `publish` writes before it renames. The count
-- is therefore what the executor did to the disk and not a figure the
-- executor keeps about itself: a writer that lost its own counter, or kept
-- one that lied, would still be seen here.
local t = ...

local NAME = "DcsEvalExecutor"

local SEP = [[\]]
local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The figures the executor publishes, the suite's own copies.
local PROBE_EVERY = 8
local QUIET_S = 3
local HEARTBEAT_S = 2

-- The wall clock this suite hands the executor, in the seconds `os.time`
-- answers. Any number does; it is advanced by hand.
local NOW = 1000000000

-- One file, written through the runner's own `io`, which is not the
-- executor's and is counted by nothing.
local function write(path, bytes)
  local fh = assert(io.open(path, "wb"))
  fh:write(bytes or "")
  fh:close()
end

-- A file's bytes through the runner's own `io`, or nil while there is none.
local function slurp(path)
  local fh = io.open(path, "rb")
  if not fh then
    return nil
  end
  local bytes = fh:read("*a")
  fh:close()
  return bytes
end

-- An envelope's headers, by name, and its body: everything after the blank
-- line, byte for byte. A file with no blank line is a failure of the suite's
-- claim that what was written is an envelope at all.
local function parsed(bytes, what)
  local blank = assert(bytes:find("\n\n", 1, true),
    what .. ": the file has a blank line ending its headers")
  local headers = {}
  for line in bytes:sub(1, blank):gmatch("([^\n]+)") do
    local name, value = line:match("^([^:]+): ?(.*)$")
    assert(name, what .. ": every header line parses: " .. line)
    headers[name] = value
  end
  return headers, bytes:sub(blank + 2)
end

-- The heartbeat on the disk, parsed. A caller that reaches here has already
-- counted the write that made it, so an absent file is a failure and not a
-- value to branch on.
local function beat(E)
  return parsed(assert(slurp(E.heartbeat), "a heartbeat is on the disk"), "heartbeat")
end

-- The lines of the events log, through the runner's own `io`.
local function lines(E)
  local out = {}
  for line in assert(slurp(E.events), "the events log is on the disk"):gmatch("([^\n]+)") do
    out[#out + 1] = line
  end
  return out
end

-- Every line of the events log whose first field is `field`.
local function marked(E, field)
  local out = {}
  for _, line in ipairs(lines(E)) do
    if line:sub(1, #field + 1) == field .. "|" then
      out[#out + 1] = line
    end
  end
  return out
end

-- The entries of a directory, sorted, dots dropped, through the model's own
-- `lfs.dir`: a listing the suite makes is not one the executor made.
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

-- A loaded state over a fresh sandbox, with the suite's clock and its write
-- counter around it. Both wrappers are installed before the load, so what a
-- load writes is counted too; the clock delegates to the model while the
-- load runs, so the stamp and the rotation are the real ones.
--
-- Returns the namespace, the state, the host, and the spy.
local function loaded(state)
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP, clock = 0 }
  local env = t.state(state, host)
  local spy = { writes = 0, now = NOW, model = true }
  spy.rawdir = env.lfs.dir
  local open, time = env.io.open, env.os.time
  env.io.open = function(path, ...)
    if type(path) == "string" and path:sub(-17) == "heartbeat.txt.tmp" then
      spy.writes = spy.writes + 1
    end
    return open(path, ...)
  end
  env.os.time = function(...)
    if spy.model then
      return time(...)
    end
    return spy.now
  end
  t.load_executor(env)()
  spy.model = false
  local E = rawget(env, NAME)
  t.eq(type(E), "table", state .. ": the namespace is published")
  t.eq(E.armed, false, state .. ": a load is asleep until a client says otherwise")
  return E, env, host, spy
end

-- The frame callback of the host under test.
local function framer(state, env, host)
  if state == "export" then
    return rawget(env, "LuaExportAfterNextFrame")
  end
  return host.callbacks.onSimulationFrame
end

-- A rare callback of the host under test, by the phase it moves to: the one
-- a mission load fires, and the one that ends a mission.
local function phaser(state, env, host, which)
  if state == "export" then
    return rawget(env, which == "in" and "LuaExportStart" or "LuaExportStop")
  end
  return host.callbacks[which == "in" and "onMissionLoadBegin" or "onSimulationStop"]
end

-- The phase each of those moves to, and the name it fires under.
local function moves(state, which)
  if state == "export" then
    return which == "in" and "sim" or "stopped",
      which == "in" and "LuaExportStart" or "LuaExportStop"
  end
  return which == "in" and "load" or "menu",
    which == "in" and "onMissionLoadBegin" or "onSimulationStop"
end

-- A request under `E.req`, by its final name, as a client publishes one.
local function request(E, id, headers, content)
  write(E.req .. SEP .. id .. ".req", headers .. "for: " .. E.stamp .. "\n\n" .. (content or ""))
end

-- What every heartbeat says about the session that wrote it, whatever made
-- it be written.
local function agrees(E, env, headers, what)
  t.eq(headers.protocol, "2", what .. ": the protocol this file is written under")
  t.eq(headers.host, E.host, what .. ": the host")
  t.eq(headers.stamp, E.stamp, what .. ": the session's stamp")
  t.eq(headers.transport, E.session, what .. ": and its directory")
  t.eq(headers.phase, E.phase, what .. ": the phase the session is in")
  t.eq(headers.armed, E.armed and "yes" or "no", what .. ": whether it is awake")
  t.eq(headers.ticks, tostring(E.tick), what .. ": the frame it was written on")
  local last = ""
  if E.last_callback_name then
    last = E.last_callback_name .. "@" .. E.last_callback_tick
  end
  t.eq(headers.last_callback, last, what .. ": the last callback other than the frame")
  t.eq(headers.callbacks, table.concat(E.callbacks, ","), what .. ": and every one seen")
  t.eq(env.lfs.attributes(E.heartbeat .. ".tmp", "mode"), nil,
    what .. ": and nothing half-written was left behind")
end

--------------------------------------------------------------------------------
-- A load writes none, and a dormant executor writes none
--------------------------------------------------------------------------------

for _, state in ipairs({ "hook", "export" }) do
  local E, env, host, spy = loaded(state)
  local frame = framer(state, env, host)
  t.eq(E.heartbeat, E.output .. [[\heartbeat.txt]], state .. ": the heartbeat is named under the output")
  t.eq(spy.writes, 0, state .. ": a load writes no heartbeat")
  t.eq(slurp(E.heartbeat), nil, state .. ": so there is no file to read")
  t.eq(entries(spy, E.output), "events.log executor.txt",
    state .. ": and the output holds what a first load leaves")
  t.eq(E.unbeaten, 0, state .. ": nothing was refused")
  -- Seeded at load off the model's own clock, because the load takes its
  -- stamp from the real one. What matters is that both are numbers a
  -- subtraction can be made against before any transition has happened.
  local seeded = E.beat_at
  t.eq(type(seeded), "number", state .. ": the beat is seeded at load")
  t.eq(E.since, seeded, state .. ": and so is the time of the last transition")

  -- The check the resolution turns on: a dormant executor over frames
  -- spanning several intervals of the beat writes nothing at all. The clock
  -- moves under it the whole way, so a writer pacing off one would fire.
  for i = 1, PROBE_EVERY * 20 do
    spy.now = NOW + i * HEARTBEAT_S
    frame()
  end
  t.eq(E.armed, false, state .. ": still asleep, because no client wrote the arm file")
  t.eq(spy.writes, 0, state .. ": and " .. (PROBE_EVERY * 20)
    .. " dormant frames across " .. (PROBE_EVERY * 20) .. " beat intervals wrote nothing")
  t.eq(slurp(E.heartbeat), nil, state .. ": there is still no heartbeat on the disk")
  t.eq(E.beat_at, seeded, state .. ": and the beat has not moved, because nothing beat")
end

--------------------------------------------------------------------------------
-- The arm writes one, and the disarm writes one
--------------------------------------------------------------------------------

for _, state in ipairs({ "hook", "export" }) do
  local E, env, host, spy = loaded(state)
  local frame = framer(state, env, host)
  write(E.arm)
  for _ = 1, PROBE_EVERY do
    frame()
  end
  t.eq(E.armed, true, state .. ": the arm file woke it")
  t.eq(spy.writes, 1, state .. ": and the waking frame wrote exactly one heartbeat")

  local headers, body = beat(E)
  t.eq(body, "", state .. ": which is an envelope with no body")
  t.eq(headers.armed, "yes", state .. ": saying it is awake")
  t.eq(headers.ticks, tostring(PROBE_EVERY), state .. ": on the frame that probed")
  t.eq(headers.since, env.os.date("%Y-%m-%d %H:%M:%S", NOW),
    state .. ": since the clock the waking frame read")
  agrees(E, env, headers, state .. " arm")

  -- Nothing to do, so the quiet window opens and the frame after the clock
  -- moves ends it. The beat is not due in that time, so the only write in
  -- this stretch is the disarm's own.
  frame()
  t.eq(spy.writes, 1, state .. ": one quiet frame inside the interval wrote nothing more")
  spy.now = NOW + QUIET_S
  frame()
  t.eq(E.armed, false, state .. ": the quiet period put it back to sleep")
  t.eq(spy.writes, 2, state .. ": and the disarm wrote exactly one more")

  headers, body = beat(E)
  t.eq(body, "", state .. ": the disarm's heartbeat has no body either")
  t.eq(headers.armed, "no", state .. ": saying it went to sleep")
  t.eq(headers.since, env.os.date("%Y-%m-%d %H:%M:%S", NOW + QUIET_S),
    state .. ": since the later transition, not the earlier one")
  agrees(E, env, headers, state .. " disarm")

  -- And a dormant executor writes nothing afterwards, however far the clock
  -- moves under it.
  for i = 1, PROBE_EVERY * 4 do
    spy.now = NOW + QUIET_S + i * HEARTBEAT_S
    frame()
  end
  t.eq(spy.writes, 2, state .. ": and it wrote nothing once it was asleep again")
end

--------------------------------------------------------------------------------
-- A phase change writes one, and says so in the events log
--------------------------------------------------------------------------------

for _, state in ipairs({ "hook", "export" }) do
  local E, env, host, spy = loaded(state)
  local frame = framer(state, env, host)
  local was = E.phase
  local to, fired = moves(state, "in")
  local before = #lines(E)
  spy.now = NOW

  phaser(state, env, host, "in")()
  t.eq(E.raised, 0, state .. ": the callback raised nothing")
  t.eq(E.phase, to, state .. ": the phase moved")
  t.eq(spy.writes, 1, state .. ": and the change wrote exactly one heartbeat")

  local headers, body = beat(E)
  t.eq(body, "", state .. ": an envelope with no body, like the others")
  t.eq(headers.phase, to, state .. ": naming the phase it moved to")
  t.eq(headers.armed, "no", state .. ": on a session that is still asleep")
  t.eq(headers.last_callback, fired .. "@" .. E.tick, state .. ": and the callback that moved it")
  agrees(E, env, headers, state .. " phase change")

  local log = lines(E)
  t.eq(#log, before + 1, state .. ": one line reached the events log")
  local changes = marked(E, "phase")
  t.eq(#changes, 1, state .. ": which is the phase line")
  t.eq(changes[1], "phase|" .. E.stamp .. "|" .. was .. "|" .. to .. "|" .. E.tick,
    state .. ": naming the session, where it came from, where it went and the frame")
  local first = changes[1]:match("^([^|]*)|")
  t.check(first ~= "B" and first ~= "O",
    state .. ": whose first field is neither dispatch marker: " .. tostring(first))

  -- The same callback again is not a second change.
  phaser(state, env, host, "in")()
  t.eq(spy.writes, 1, state .. ": a callback repeating the phase it is in wrote nothing")
  t.eq(#lines(E), before + 1, state .. ": and appended nothing")
  t.eq(E.raised, 0, state .. ": still nothing raised")

  -- A change on an armed session writes one too, and the frame carries on.
  write(E.arm)
  for _ = 1, PROBE_EVERY do
    frame()
  end
  t.eq(E.armed, true, state .. ": the arm file woke it")
  t.eq(spy.writes, 2, state .. ": the arm wrote the second")
  local out, back = moves(state, "out")
  phaser(state, env, host, "out")()
  t.eq(E.phase, out, state .. ": an armed session changed phase too")
  t.eq(spy.writes, 3, state .. ": which wrote the third")
  headers = beat(E)
  t.eq(headers.armed, "yes", state .. ": saying it is awake")
  t.eq(headers.phase, out, state .. ": in the phase it moved to")
  t.eq(headers.last_callback, back .. "@" .. E.tick, state .. ": under the callback that moved it")
  t.eq(#marked(E, "phase"), 2, state .. ": and the events log holds both changes")

  request(E, "1-ping", "op: ping\n")
  frame()
  t.check(slurp(E.res .. SEP .. "1-ping.res"), state .. ": and the frame after it still answers")
end
