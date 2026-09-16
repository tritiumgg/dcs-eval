-- `a_do_script`'s off-by-one, reproduced under the reference interpreter
-- so the executor's correction is exercised rather than asserted.
--
-- `a_do_script` shifts what the chunk it runs returns by one: `v1 … vN`
-- arrives as `nil, v1 … v(N-1)`, so the first slot is a spurious nil and
-- the last value is dropped, and a lone value is lost entirely. It was
-- measured on DCS 2.9.29.27278, calling from `mission` and reading
-- `pcall`'s whole result list: `1, 2` came back `nil, 1`, one value of any
-- type came back `nil`, and `'x', 0` came back as three slots under
-- `pcall`. The correction is two halves: the far chunk ends with a
-- sacrificial `0`, and the near chunk reads the payload out of slot 2.
--
-- This suite keeps its own `a_do_script`, apart from the model in
-- `executor/a_do_script`, and first holds it to that measurement slot for
-- slot, counting with `select("#")`, because `#` does not count a trailing
-- nil and would hide the very value the shift drops. What a chunk
-- returning nothing comes back as was never counted, so it is not pinned
-- here. Then it runs the executor's two hops over that `a_do_script`: a
-- lone value crosses because the far chunk returned it and a `0`, and
-- without the `0` the same far chunk, run under the same shift, loses it
-- and the reply says so under `stage: a_do_script` rather than reading as
-- an empty result. The far chunk's own refusal, a second return of its
-- own, crosses the shift the same way. A run against DCS that disagrees
-- with this file has measured DCS.
--
-- The mutations this suite exists to catch. Drop the far chunk's trailing
-- `0`, and a lone value is dropped: the correction check reads a refusal
-- where it wants `ok`. Drop the `0` after the far chunk's refusal, and
-- that refusal is dropped. Read slot 1 in the near chunk, and it reads the
-- shift's nil. Stop this suite's `a_do_script` shifting, and the
-- measurement checks read which slot moved.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

local function pack(...)
  return { n = select("#", ...), ... }
end

-- The shift as measured: `v1 … vN` answered as `nil, v1 … v(N-1)`, N slots.
local function shift(values)
  local out = { n = values.n }
  for i = 1, values.n - 1 do
    out[i + 1] = values[i]
  end
  return out
end

-- An `a_do_script` that compiles the source in `far`, runs it with the
-- arguments it was passed after the source, and answers with what
-- `keep` makes of the return list, shifted. Each call is recorded in
-- `crossings`: the source, the arguments, what the chunk returned, and
-- what it answered. `keep` defaults to the whole list.
local function measured(far, crossings, keep)
  return function(source, ...)
    local record = { source = source, args = pack(...) }
    crossings[#crossings + 1] = record
    local fn = assert(loadstring(source, "=far"))
    setfenv(fn, far)
    record.returned = pack(fn(...))
    local kept = keep and keep(record.returned) or record.returned
    record.answered = shift(kept)
    return unpack(record.answered, 1, record.answered.n)
  end
end

-- The first value alone, as a far chunk without its sacrificial `0` returns.
local function first(values)
  return { n = 1, values[1] }
end

-- A state over a fresh sandbox with the executor loaded into it, as the
-- hook host. Returns the namespace, the state, the host and the frame.
local function loaded()
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP }
  local env = t.state("hook", host)
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(type(E), "table", "the executor loaded over hook")
  return E, env, host, host.callbacks.onSimulationFrame
end

-- The first hop: `net.dostring_in` running what it was handed in `mission`,
-- the answer of each call recorded in `hops`.
local function carrier(env, mission, hops)
  env.net.dostring_in = function(state, chunk)
    t.eq(state, "mission", "the first hop is into mission")
    local fn = assert(loadstring(chunk, "=near"))
    setfenv(fn, mission)
    hops[#hops + 1] = fn()
    return hops[#hops]
  end
end

-- A hook host with a mission loaded and `a_do_script` built by `build`
-- over a modelled `missionscripting`. Returns the namespace, the state,
-- the frame, and the two records.
local function opened(build)
  local E, env, host, frame = loaded()
  local hops, crossings = {}, {}
  local mission = t.state("mission", { mission_loaded = true })
  mission.a_do_script = build(t.state("missionscripting", host), crossings)
  carrier(env, mission, hops)
  return E, env, frame, hops, crossings
end

-- The entries of a directory as the model lists them, dots dropped.
local function entries(env, dir)
  local names = {}
  for name in env.lfs.dir(dir) do
    if name ~= "." and name ~= ".." then
      names[#names + 1] = name
    end
  end
  return table.concat(names, " ")
end

-- An `eval` into `missionscripting`, answered on one frame and read back
-- with the suite's own reader: the values by name, and the body.
local function eval(E, frame, id, body)
  local fh = assert(io.open(E.req .. "\\" .. id .. ".req", "wb"))
  fh:write("op: eval\nfor: " .. E.stamp .. "\nstate: missionscripting\n\n" .. body)
  fh:close()
  frame()
  fh = assert(io.open(E.res .. "\\" .. id .. ".res", "rb"), id .. ": no reply on the disk")
  local bytes = fh:read("*a")
  fh:close()
  local blank = bytes:find("\n\n", 1, true)
  t.check(blank, id .. ": the reply has a blank line ending its headers")
  local values = {}
  for line in bytes:sub(1, blank):gmatch("([^\n]*)\n") do
    local name, value = line:match("^([A-Za-z0-9_%-]+): (.*)$")
    t.check(name, id .. ": every header line reads name: value, but one reads " .. line)
    values[name] = value
  end
  return values, bytes:sub(blank + 2)
end

--------------------------------------------------------------------------------
-- The model is the shift as measured
--------------------------------------------------------------------------------

do
  local crossings = {}
  local a_do_script = measured(t.state("missionscripting", {}), crossings)

  for n = 2, 4 do
    local values = {}
    for i = 1, n do
      values[i] = tostring(i)
    end
    local got = pack(a_do_script("return " .. table.concat(values, ", ")))
    local what = "return " .. table.concat(values, ", ")
    t.eq(got.n, n, what .. ": answers as many slots as were returned")
    t.eq(got[1], nil, what .. ": slot 1 is a spurious nil")
    for i = 2, n do
      t.eq(got[i], i - 1, what .. ": slot " .. i .. " holds value " .. (i - 1))
    end
  end

  local lone = {
    { "number", "return 1" },
    { "string", 'return "lone"' },
    { "boolean", "return true" },
    { "table", "return {}" },
  }
  for _, case in ipairs(lone) do
    local got = pack(a_do_script(case[2]))
    t.eq(got.n, 1, case[1] .. ": a lone value answers one slot")
    t.eq(got[1], nil, case[1] .. ": and it is nil, the value lost")
    t.eq(type(crossings[#crossings].returned[1]), case[1], case[1] .. ": which the chunk did return")
  end

  local got = pack(a_do_script('return "p", 0'))
  t.eq(got.n, 2, "sacrificial: a payload and a 0 answer two slots")
  t.eq(got[1], nil, "sacrificial: nil in slot 1")
  t.eq(got[2], "p", "sacrificial: and the payload in slot 2")
end

--------------------------------------------------------------------------------
-- The correction carries a lone value across
--------------------------------------------------------------------------------

do
  local E, env, frame, _, crossings = opened(function(far, crossings)
    return measured(far, crossings)
  end)

  local cases = {
    { "string", 'return "lone"', "string", "lone" },
    { "nil", "return nil", "nil", "" },
  }
  for i, case in ipairs(cases) do
    local what = case[1]
    local v, body = eval(E, frame, "1-" .. i, case[2])
    t.eq(v.status, "ok", what .. ": a lone value through the shift is answered: " .. body)
    t.eq(v.result_type, case[3], what .. ": typed")
    t.eq(body, case[4], what .. ": with its body")
    local crossing = crossings[#crossings]
    t.eq(crossing.returned.n, 2, what .. ": because the far chunk returned two slots")
    t.eq(crossing.returned[2], 0, what .. ": the second the sacrificial 0")
    t.eq(crossing.answered.n, 2, what .. ": a_do_script answered two slots")
    t.eq(crossing.answered[1], nil, what .. ": nil in slot 1")
    t.eq(crossing.answered[2], crossing.returned[1], what .. ": and the payload in slot 2")
    t.eq(crossing.answered[2], "ok\n" .. case[3] .. "\n" .. case[4], what .. ": which is what the reply carries")
  end

  t.eq(entries(env, E.req), "", "correction: every request is taken")
  t.eq(E.raised, 0, "correction: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- Without the sacrificial 0, a lone value is dropped
--------------------------------------------------------------------------------

do
  local E, env, frame, hops, crossings = opened(function(far, crossings)
    return measured(far, crossings, first)
  end)

  local v, body = eval(E, frame, "2-a", 'return "lone"')
  local crossing = crossings[1]
  t.eq(crossing.returned[1], "ok\nstring\nlone", "dropped: the far chunk ran and produced the payload")
  t.eq(crossing.answered.n, 1, "dropped: without the 0, a_do_script answered one slot")
  t.eq(crossing.answered[1], nil, "dropped: and it is nil, the payload lost to the shift")
  t.eq(v.status, "error", "dropped: a lost payload is not an ok")
  t.eq(v.stage, "a_do_script", "dropped: it is stage a_do_script")
  t.eq(body, "slot 1 is nil and slot 2 is nil, where a_do_script's shift puts a nil in slot 1"
    .. " and the string payload in slot 2", "dropped: naming both slots")
  t.eq(hops[1], "a_do_script\n\n" .. body, "dropped: refused inside mission")

  t.eq(entries(env, E.req), "", "dropped: every request is taken")
  t.eq(E.raised, 0, "dropped: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The far chunk's own refusal crosses the shift
--------------------------------------------------------------------------------

do
  -- An `a_do_script` that runs the far chunk under the shift and hands it
  -- none of its arguments.
  local E, env, frame, _, crossings = opened(function(far, crossings)
    local a_do_script = measured(far, crossings)
    return function(source)
      return a_do_script(source)
    end
  end)

  local v, body = eval(E, frame, "3-a", "return 1")
  local crossing = crossings[1]
  t.eq(crossing.args.n, 0, "no arguments: the far chunk was handed none")
  t.eq(crossing.returned.n, 2, "no arguments: its refusal is two slots, the second the 0")
  t.eq(v.status, "error", "no arguments: answered as an error")
  t.eq(v.stage, "a_do_script", "no arguments: under stage a_do_script")
  t.eq(body, "the far chunk was handed a nil body and a nil chunkname, where a_do_script passes its"
    .. " arguments on as strings", "no arguments: the far chunk's own message crossed the shift")

  t.eq(entries(env, E.req), "", "no arguments: every request is taken")
  t.eq(E.raised, 0, "no arguments: nothing reached the guard")
end
