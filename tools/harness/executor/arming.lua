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
-- and one when the iterator it returned first hands back nil. Each hook
-- names the listing of the tick it wants and fires on that one alone: a
-- disarming frame lists twice, the armed path's own listing and then the
-- disarm's, and a hook that could not tell them apart would fire on the
-- first and prove nothing about the order the second one is in.
--
-- What the two hooks prove, which is the race. Publishing a request from
-- the `after` hook puts it on the disk after the disarm's listing was
-- taken, which is the moment the specification's proof turns on: the
-- executor sees nothing and sleeps, and the client, finding the arm file
-- already gone, creates it, so the next probe wakes and answers. Reverse
-- the removal and the listing in the executor and the same client finds the
-- arm file still there, creates nothing, and the removal that follows
-- strands its request in front of an executor with nothing left to wake it.
-- Publishing from the `before` hook is the other end: the request is in the
-- listing, so the executor puts the arm file back and stays awake.
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
  local spy = { dirs = 0, times = 0, now = NOW, model = true }
  local dir, time = env.lfs.dir, env.os.time
  spy.rawdir = dir
  -- A hook fires on the listing it names and on no other, and is gone once
  -- it has: a hook that fired on whichever listing came first would fire on
  -- the armed path's own and say nothing about the disarm's. `spy.on` sets
  -- one and `spy.fired` says whether it went off, which is the check that
  -- the listing it named ever happened.
  local hooks = {}
  spy.on = function(which, at, fn)
    hooks[which] = { at = at, fn = fn }
  end
  spy.fired = function(which)
    return hooks[which] == nil
  end
  local function fire(which)
    local h = hooks[which]
    if h and spy.dirs == h.at then
      hooks[which] = nil
      h.fn(spy.dirs)
    end
  end
  env.lfs.dir = function(path)
    spy.dirs = spy.dirs + 1
    fire("before")
    local iter = dir(path)
    local drained = false
    return function()
      local name = iter()
      if name == nil and not drained then
        drained = true
        fire("after")
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
  t.eq(E.armed, false, state .. ": a load is asleep until a client says otherwise")
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

  local arms = marked(E, "arm")
  t.eq(#arms, 1, "the wake wrote one arm line")
  t.eq(arms[1], "arm|" .. E.stamp .. "|" .. PROBE_EVERY, "which names the session and the tick it woke on")
  local first = arms[1]:match("^([^|]*)|")
  t.check(first ~= "B" and first ~= "O", "whose first field is neither marker: " .. tostring(first))
end

--------------------------------------------------------------------------------
-- A frame that finds nothing writes nothing
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("hook")
  local frame = framer("hook", env, host, spy)
  local before = #lines(E)
  for _ = 1, 100 do
    frame()
  end
  t.eq(E.armed, false, "a hundred dormant frames found no arm file")
  t.eq(#lines(E), before, "and wrote not one line")
  t.eq(#marked(E, "arm"), 0, "no arm line among them")
end

--------------------------------------------------------------------------------
-- An abandoned arm file costs one quiet period and then nothing
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("hook")
  local frame = framer("hook", env, host, spy)
  -- A client that created the arm file and died before publishing anything.
  write(E.arm)
  for _ = 1, PROBE_EVERY do
    frame()
  end
  t.eq(E.armed, true, "the abandoned file woke it")

  -- The first armed frame with nothing in front of it opens the window; the
  -- clock is the suite's, so the period costs no real time.
  frame()
  t.eq(E.armed, true, "one quiet frame is not a quiet period")
  t.eq(E.quiet_since, NOW, "and the window opened at the clock the frame read")
  spy.now = NOW + QUIET_S
  frame()
  t.eq(E.armed, false, "the quiet period ended it")
  t.eq(spy.dirs, 2, "the disarming tick listed twice: the frame's own, then the disarm's")
  t.eq(env.lfs.attributes(E.arm, "mode"), nil, "the executor removed the arm file it did not create")
  t.eq(E.quiet_since, nil, "and closed the window behind it")

  local log = lines(E)
  local arm, disarmed
  for i, line in ipairs(log) do
    local first = line:match("^([^|]*)|")
    if first == "arm" then
      arm = i
    elseif first == "disarm" then
      disarmed = i
    end
    if first == "arm" or first == "disarm" then
      t.check(first ~= "B" and first ~= "O",
        "a transition's first field is neither marker: " .. tostring(first))
    end
  end
  t.eq(#marked(E, "arm"), 1, "one arm line")
  t.eq(#marked(E, "disarm"), 1, "one disarm line")
  t.check(arm and disarmed and arm < disarmed, "the arm line is before the disarm line")
  t.eq(log[disarmed], "disarm|" .. E.stamp .. "|" .. E.tick, "which names the session and the tick it slept on")

  -- And a dormant executor with no arm file stays where it is.
  for _ = 1, PROBE_EVERY * 4 do
    frame()
  end
  t.eq(E.armed, false, "it stayed asleep, because the file it removed is not back")
  t.eq(#marked(E, "arm"), 1, "and wrote no second arm line")
end

--------------------------------------------------------------------------------
-- A request every frame keeps it awake, and costs no clock read
--------------------------------------------------------------------------------

do
  local E, env, host, spy = loaded("hook")
  local frame = framer("hook", env, host, spy)
  write(E.arm)
  for _ = 1, PROBE_EVERY do
    frame()
  end
  t.eq(E.armed, true, "armed")
  for i = 1, QUIET_S * 4 do
    request(E, "4-" .. i, "op: ping\n")
    spy.times = 0
    -- Far more than a quiet period between frames: what holds it awake is
    -- the work in front of it and nothing about the clock.
    spy.now = spy.now + QUIET_S * 2
    frame()
    t.eq(spy.times, 0, "frame " .. i .. ": an armed frame with a request listed reads no wall clock")
    t.eq(E.armed, true, "frame " .. i .. ": and stays awake")
  end
  t.eq(#marked(E, "disarm"), 0, "nothing disarmed while it had work")
  t.eq(env.lfs.attributes(E.arm, "mode"), "file", "and the arm file is where the client left it")
end

--------------------------------------------------------------------------------
-- The disarm cannot strand a request
--------------------------------------------------------------------------------

-- What the client does in `send`, in that order: the request under its final
-- name, then the arm file if it is not already there. The client never
-- removes it.
local function client(E, env, id)
  request(E, id, "op: ping\n")
  if env.lfs.attributes(E.arm, "mode") == nil then
    write(E.arm)
  end
end

-- An executor on the frame before the one it will disarm on: armed off the
-- arm file, one quiet frame to open the window, and the suite's clock moved
-- a whole quiet period on. The next frame is the disarming one.
local function quieted(state)
  local E, env, host, spy = loaded(state)
  local frame = framer(state, env, host, spy)
  write(E.arm)
  for _ = 1, PROBE_EVERY + 1 do
    frame()
  end
  t.eq(E.armed, true, state .. ": armed, and one quiet frame in")
  t.eq(E.quiet_since, NOW, state .. ": with the window open")
  spy.now = NOW + QUIET_S
  return E, env, host, spy, frame
end

-- What a failed race says, because "no reply" alone does not say which of
-- the three ways it went wrong.
local function said(E, spy)
  return " (tick " .. E.tick .. ", armed " .. tostring(E.armed)
    .. ", listings " .. spy.dirs .. ", req [" .. entries(spy, E.req)
    .. "], res [" .. entries(spy, E.res) .. "])"
end

do
  local E, env, _, spy, frame = quieted("hook")
  -- The client publishes after the disarm's listing was taken, which is the
  -- case the order exists for.
  spy.on("after", 2, function(n)
    t.eq(n, 2, "the hook fired on the disarm's listing and not the frame's own")
    client(E, env, "5-ping")
  end)
  frame()
  t.check(spy.fired("after"), "the disarm made its second listing" .. said(E, spy))
  t.eq(spy.dirs, 2, "and made no third" .. said(E, spy))
  t.eq(E.armed, false, "it slept: the request was not in the listing it took" .. said(E, spy))
  t.eq(#marked(E, "disarm"), 1, "and said so once")
  t.eq(env.lfs.attributes(E.arm, "mode"), "file",
    "the client found the arm file gone and put it back" .. said(E, spy))

  local at
  for i = 1, PROBE_EVERY + 1 do
    frame()
    if not at and published(E, "5-ping") then
      at = i
    end
  end
  t.check(at, "the request published into the disarm was answered" .. said(E, spy))
  t.eq(E.armed, true, "by an executor the arm file woke again")
  t.eq(#marked(E, "arm"), 2, "which is the second arm of the session")
end

--------------------------------------------------------------------------------
-- A request in the disarm's own listing keeps it awake
--------------------------------------------------------------------------------

do
  local E, env, _, spy, frame = quieted("hook")
  -- The other end of the same race: published before the disarm's listing
  -- is taken, so it is in it.
  spy.on("before", 2, function(n)
    t.eq(n, 2, "the hook fired on the disarm's listing and not the frame's own")
    client(E, env, "6-ping")
  end)
  frame()
  t.check(spy.fired("before"), "the disarm made its second listing" .. said(E, spy))
  t.eq(E.armed, true, "and stayed awake, because that listing held a request" .. said(E, spy))
  t.eq(#marked(E, "disarm"), 0, "so nothing was recorded as a disarm")
  t.eq(env.lfs.attributes(E.arm, "mode"), "file", "and the arm file is back on the disk")
  t.eq(E.quiet_since, nil, "with the quiet window closed behind it")

  frame()
  t.check(published(E, "6-ping"), "the ordinary frame after it answered" .. said(E, spy))
  t.eq(#marked(E, "arm"), 1, "and nothing woke, because nothing had slept")
end

--------------------------------------------------------------------------------
-- A global left in a target state survives a sleep
--------------------------------------------------------------------------------

do
  local E, env, _, spy, frame = quieted("hook")
  request(E, "2-set", "op: eval\nstate: hook\n", "ARMING_MARK = 'kept'; return 'set'")
  frame()
  t.eq(body(E, "2-set"), "set", "the first chunk ran while armed")
  t.eq(E.armed, true, "and the work closed the quiet window" .. said(E, spy))

  -- A real disarm, not a hand-set flag: the window opens on the next quiet
  -- frame and the one after the clock moves ends it.
  frame()
  spy.now = spy.now + QUIET_S
  frame()
  t.eq(E.armed, false, "then it slept" .. said(E, spy))

  request(E, "2-get", "op: eval\nstate: hook\n", "return ARMING_MARK")
  write(E.arm)
  for _ = 1, PROBE_EVERY + 1 do
    frame()
  end
  t.eq(E.armed, true, "the arm file woke it again")
  t.eq(body(E, "2-get"), "kept", "and the global the first chunk set is still there")
end
