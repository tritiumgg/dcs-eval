-- Arming and disarming: what wakes the executor, what puts it back to
-- sleep, and what neither may lose. `executor/dormant` holds the cost of a
-- frame that is asleep — the byte count, the one probe in eight — and this
-- suite says nothing about it; what is here is the transitions.
--
-- What is proved. A request published while the executor is dormant, with
-- the arm file beside it the way a client writes them, is answered within
-- `PROBE_EVERY` + 1 frames: the probing frame arms, the frame after it
-- lists. A global a chunk leaves in a target state is still there for a
-- chunk that runs after the executor has slept and woken, because sleeping
-- touches no state but the executor's own.
--
-- The clock and the listings are the suite's, not the machine's. `os.time`
-- is wrapped: it delegates to the model while the executor loads, so the
-- session stamp is a real one, and answers a number this suite advances by
-- hand afterwards, so no check here waits on a real second and a frame that
-- reads no clock can be told from one that does. `lfs.dir` is wrapped by a
-- per-tick counter with two one-shot hooks, one fired on entry to the call
-- and one when the iterator it returned first hands back nil. The counter
-- is what tells the two listings of a disarming frame apart, and a hook
-- that could not tell them apart would fire on the wrong one and prove
-- nothing.
--
-- The model's `lfs.dir` takes its snapshot inside the call, so a file
-- created from the `after` hook is provably outside the listing being
-- iterated and one created from the `before` hook is provably inside it.
-- That is what makes the two hooks two different claims. Were the model
-- ever made lazy, the `after` hook would still say what it says and the
-- `before` hook would become the weaker of the two.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The figures the executor publishes, the suite's own copies.
local PROBE_EVERY = 8
local QUIET_S = 3

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

-- A published reply's bytes, or nil while there is none. A reply arrives by
-- rename, so one that opens is whole.
local function published(E, id)
  local fh = io.open(E.res .. "\\" .. id .. ".res", "rb")
  if not fh then
    return nil
  end
  local bytes = fh:read("*a")
  fh:close()
  return bytes
end

-- A reply's body, everything after the blank line that ends its headers.
local function body(E, id)
  local bytes = assert(published(E, id), id .. ": a reply is on the disk")
  local blank = assert(bytes:find("\n\n", 1, true),
    id .. ": the reply has a blank line ending its headers")
  return bytes:sub(blank + 2)
end

-- The lines of the events log, through the runner's own `io`. The trailing
-- newline leaves no empty last line.
local function lines(E)
  local fh = assert(io.open(E.events, "rb"), "the events log is on the disk")
  local bytes = fh:read("*a")
  fh:close()
  local out = {}
  for line in bytes:gmatch("([^\n]+)") do
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

-- The entries of a directory, sorted, dots dropped, through the `lfs.dir`
-- the model made rather than the counting wrapper: a listing the suite makes
-- is not one the executor made.
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

-- A loaded state over a fresh sandbox with the suite's clock and listing
-- counter around it. Both wrappers are installed before the load and
-- delegate to the model while it runs, so the stamp, the rotation and the
-- sweep are the real ones; afterwards the clock answers `spy.now` and the
-- counters are zeroed.
--
-- Returns the namespace, the state, the host, and the spy.
local function loaded(state)
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP, clock = 0 }
  local env = t.state(state, host)
  local spy = { dirs = 0, times = 0, now = NOW, model = true, before = nil, after = nil }
  local dir, time = env.lfs.dir, env.os.time
  spy.rawdir = dir
  env.lfs.dir = function(path)
    spy.dirs = spy.dirs + 1
    local before = spy.before
    if before then
      spy.before = nil
      before(spy.dirs)
    end
    local iter = dir(path)
    local drained = false
    return function()
      local name = iter()
      if name == nil and not drained then
        drained = true
        local after = spy.after
        if after then
          spy.after = nil
          after(spy.dirs)
        end
      end
      return name
    end
  end
  env.os.time = function(...)
    spy.times = spy.times + 1
    if spy.model then
      return time(...)
    end
    return spy.now
  end
  t.load_executor(env)()
  spy.model = false
  spy.dirs, spy.times = 0, 0
  local E = rawget(env, NAME)
  t.eq(type(E), "table", state .. ": the namespace is published")
  return E, env, host, spy
end

-- The frame callback, wrapped so `spy.dirs` counts the listings of one tick
-- and no more: "the disarm's listing" is the second of a tick, and a hook
-- that means it asserts the number rather than assuming it.
local function framer(state, env, host, spy)
  local frame
  if state == "export" then
    frame = rawget(env, "LuaExportAfterNextFrame")
  else
    frame = host.callbacks.onSimulationFrame
  end
  return function()
    spy.dirs = 0
    return frame()
  end
end

-- A request under `E.req`, by its final name, as a client publishes one.
local function request(E, id, headers, content)
  write(E.req .. "\\" .. id .. ".req", headers .. "for: " .. E.stamp .. "\n\n" .. (content or ""))
end

--------------------------------------------------------------------------------
-- A request published while dormant is answered within PROBE_EVERY + 1 ticks
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("hook")
  local frame = framer("hook", env, host, spy)
  E.armed = false
  -- The order a client sends in: the request under its final name, then the
  -- arm file.
  request(E, "1-ping", "op: ping\n")
  write(E.arm)
  local at
  for i = 1, PROBE_EVERY + 1 do
    frame()
    if not at and published(E, "1-ping") then
      at = i
    end
  end
  t.check(at, "the dormant executor answered, at tick " .. tostring(at)
    .. " (armed " .. tostring(E.armed) .. ", req [" .. entries(spy, E.req)
    .. "], res [" .. entries(spy, E.res) .. "])")
  t.eq(at, PROBE_EVERY + 1, "the probing frame arms and the frame after it answers")
  t.eq(E.armed, true, "and it is awake afterwards")
  t.eq(entries(spy, E.req), "", "the request was taken")
  t.eq(spy.dirs, 1, "an ordinary armed tick lists once")
end

--------------------------------------------------------------------------------
-- A global left in a target state survives a sleep
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("hook")
  local frame = framer("hook", env, host, spy)
  request(E, "2-set", "op: eval\nstate: hook\n", "ARMING_MARK = 'kept'; return 'set'")
  frame()
  t.eq(body(E, "2-set"), "set", "the first chunk ran while armed")

  E.armed = false
  for _ = 1, PROBE_EVERY * 3 do
    frame()
  end
  t.eq(E.armed, false, "it slept, because nothing armed it")

  request(E, "2-get", "op: eval\nstate: hook\n", "return ARMING_MARK")
  write(E.arm)
  for _ = 1, PROBE_EVERY + 1 do
    frame()
  end
  t.eq(E.armed, true, "the arm file woke it again")
  t.eq(body(E, "2-get"), "kept", "and the global the first chunk set is still there")
end
