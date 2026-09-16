-- The `eval` op through the mission door: `net.dostring_in` into
-- `mission` carrying the door chunk, which calls `a_do_script` with the
-- far chunk and the request's body and `chunkname` as arguments; the far
-- chunk compiles and runs the body in `missionscripting`, converts the
-- result there, and returns the three fields and a sacrificial `0`;
-- `a_do_script` shifts that by one, and the door reads slot 2.
--
-- The models evaluate nothing, on purpose, so this suite installs both
-- hops over them. The first is the carrier `executor/dostring` installs:
-- it compiles what the executor handed `net.dostring_in` with the
-- runner's own `loadstring` and runs it with a modelled `mission` as its
-- globals. The second is a door of the suite's own in that `mission`: it
-- records what it was passed, compiles the far chunk in a modelled
-- `missionscripting`, runs it with the arguments, and answers with the
-- return list shifted as DCS shifts it, `v1 … vN` as `nil, v1 … v(N-1)`.
-- What that proves is the door under the reference interpreter against
-- the shift as it was measured; what DCS does with it is a live install's
-- to say, and a door that is not the measured one is refused here and
-- read as such there.
--
-- What is proved. A value crosses as a string only: every type a chunk
-- returns comes back typed and printed, a table as an empty body, and
-- what the door was handed in slot 2 was a string each time. The door
-- reads slot 2: the far chunk returned the payload and `0`, and a door
-- that does not shift is refused with the types of both slots named. The
-- backstop: a payload in slot 2 that is not a string, a table among them,
-- is refused under `stage: door` without a key of it read. The body
-- crosses into `mission` once, as a `%q` literal, and into
-- `missionscripting` as an argument; the far chunk is the same bytes for
-- every request and holds nothing a requester wrote. Line 47 is true
-- through both hops. The chunk runs in `missionscripting`'s globals. The
-- ceiling is applied beyond the door. With no mission loaded the door is
-- `door-shut`, read at the moment of use: loaded, a request crosses, and
-- unloaded again, the next is shut. A door that raises, a far chunk
-- handed no arguments, and the door chunk's own raise are each answered.
-- The first hop's answers are the carrier's: `refused`, `invalid-state`,
-- the model's empty answer, and a host without `net`. Every reply
-- carries `carrier: a_do_script` and `via: mission`. The export host
-- serves no door.
--
-- The mutations this suite exists to catch. Return the chunk's raw value
-- from the far chunk instead of the converted string, and the first
-- crossing, a table, is refused by the backstop where it wants `ok`. Drop
-- the backstop, and a door that does not shift is read as an unshaped
-- answer rather than `stage: door`. Read slot 1 instead of slot 2, or drop
-- the sacrificial `0`, and that first crossing reads a nil payload. Put
-- the body in the far chunk, and it appears twice in what crosses into
-- `mission`. Keep `a_do_script` from a previous crossing, and a door
-- replaced between requests is not the one called. Drop the door's two
-- headers from a refusal, or read `door-shut` as an error, and the header
-- checks read which.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The reply's headers, in order. The suite's own copy, kept apart from the
-- executor's on purpose. Every reply through the door ends with the two
-- that say so.
local HEAD = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "carrier", "via" }
local OK = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "result_type", "chunkname", "carrier", "via" }
local ERR = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage", "chunkname", "carrier", "via" }
local OVERSIZE = {
  "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage", "chunkname", "result_bytes", "carrier", "via",
}
local PLAIN = { "status", "protocol", "host", "stamp", "phase", "id", "tick" }

-- The entries of a directory as the model lists them, sorted, dots dropped.
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

-- A state over a fresh sandbox with the executor loaded into it. Returns
-- the namespace, the state, the host and the frame callback.
local function loaded(state)
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP }
  local env = t.state(state, host)
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(type(E), "table", "the executor loaded over " .. state)
  local frame
  if state == "hook" then
    frame = host.callbacks.onSimulationFrame
  else
    frame = rawget(env, "LuaExportAfterNextFrame")
  end
  return E, env, host, frame
end

-- `net.dostring_in` in the hook state, replaced.
local function answering(env, fn)
  env.net.dostring_in = fn
end

-- The first hop: `net.dostring_in` running what it was handed in the model
-- `targets` names, and "Invalid state name" for any other state. Each call
-- is recorded in `hops`.
local function carrier(env, targets, hops)
  answering(env, function(state, chunk)
    local record = { state = state, chunk = chunk }
    hops[#hops + 1] = record
    local target = targets[state]
    if not target then
      record.answered = "Invalid state name"
      return "Invalid state name"
    end
    local fn = assert(loadstring(chunk, "=door"))
    setfenv(fn, target)
    record.answered = fn()
    return record.answered
  end)
end

local function pack(...)
  return { n = select("#", ...), ... }
end

-- The second hop: an `a_do_script` that compiles the far chunk in `far`,
-- runs it with the arguments it was passed after the source, and answers
-- with what it returned shifted by one, `v1 … vN` as `nil, v1 … v(N-1)`,
-- as DCS was measured to. Each call is recorded in `crossings`: the
-- source, the arguments, what the far chunk returned, and what the door
-- answered. `shift` false answers what the far chunk returned unshifted,
-- a DCS build that corrected it.
local function door(far, crossings, shift)
  return function(source, ...)
    local record = { source = source, args = pack(...) }
    crossings[#crossings + 1] = record
    local fn = assert(loadstring(source, "=far"))
    setfenv(fn, far)
    record.returned = pack(fn(...))
    local out = record.returned
    if shift ~= false then
      out = { n = record.returned.n }
      for i = 1, record.returned.n - 1 do
        out[i + 1] = record.returned[i]
      end
    end
    record.answered = out
    return unpack(out, 1, out.n)
  end
end

-- `a_do_script` in a modelled `mission`, as a mission loading puts it
-- there and unloading clears it.
local function install(mission, fn)
  mission.a_do_script = fn
end

-- A hook host with both hops in place and a mission loaded. Returns the
-- namespace, the hook state, the frame, the `mission` and
-- `missionscripting` models, and the two records.
local function opened()
  local E, env, host, frame = loaded("hook")
  local hops, crossings = {}, {}
  local mission = t.state("mission", { mission_loaded = true })
  local far = t.state("missionscripting", host)
  install(mission, door(far, crossings))
  carrier(env, { mission = mission }, hops)
  return E, env, frame, mission, far, hops, crossings
end

-- One request under `E.req`, written through the runner's own `io`.
local function request(E, name, content)
  local fh = assert(io.open(E.req .. "\\" .. name, "wb"))
  fh:write(content)
  fh:close()
end

-- A reply read back with the suite's own reader: the names in the order
-- written, the values by name, and the body.
local function read(E, id)
  local fh = assert(io.open(E.res .. "\\" .. id .. ".res", "rb"), id .. ": no reply on the disk")
  local bytes = fh:read("*a")
  fh:close()
  local blank = bytes:find("\n\n", 1, true)
  t.check(blank, id .. ": the reply has a blank line ending its headers")
  local order, values = {}, {}
  for line in bytes:sub(1, blank):gmatch("([^\n]*)\n") do
    local name, value = line:match("^([A-Za-z0-9_%-]+): (.*)$")
    t.check(name, id .. ": every header line reads name: value, but one reads " .. line)
    order[#order + 1] = name
    values[name] = value
  end
  return order, values, bytes:sub(blank + 2)
end

-- The names of `order` against `want`, one line, so a missing or extra
-- header reads as which.
local function fields(order, want, what)
  local w, got = table.concat(want, " "), table.concat(order, " ")
  t.eq(#order, #want, what .. ": every header is present, and no other: " .. got)
  t.eq(got, w, what .. ": and in the wire's order")
end

-- An `eval` into `missionscripting` for the session under `id`, with
-- `extra` header lines after `state`, answered on one frame and read back.
local function eval(E, frame, id, extra, body)
  request(E, id .. ".req", "op: eval\nfor: " .. E.stamp .. "\nstate: missionscripting\n" .. (extra or "") .. "\n" .. body)
  frame()
  return read(E, id)
end

-- A body of 46 comment lines and `line` as its line 47, LF endings.
local function at47(line)
  local lines = {}
  for i = 1, 46 do
    lines[i] = "-- line " .. i
  end
  lines[47] = line
  return table.concat(lines, "\n")
end

-- The handshake's value for one header, read with the suite's own reader.
local function published(E, name)
  local fh = assert(io.open(E.handshake, "rb"))
  local bytes = fh:read("*a")
  fh:close()
  return bytes:match("\n" .. name .. ": ([^\n]*)\n")
end

--------------------------------------------------------------------------------
-- A value crosses as a string only
--------------------------------------------------------------------------------

do
  local E, env, frame, _, _, hops, crossings = opened()

  -- A table first: converted beyond the door, so the backstop at the door
  -- has nothing to refuse. A far chunk that handed the table back
  -- unconverted is refused there, and this is where that reads red.
  local order, v, body = eval(E, frame, "1-0", "", "return { 1, 2, 3 }")
  t.eq(v.stage, nil, "table: the string-only backstop had nothing to refuse: " .. body)
  t.eq(type(crossings[1].returned[1]), "string", "table: the far chunk returned a string for it")
  fields(order, OK, "table")
  t.eq(v.result_type, "table", "table: typed beyond the door")
  t.eq(body, "", "table: with an empty body")
  table.remove(hops, 1)
  table.remove(crossings, 1)

  order, v, body = eval(E, frame, "1-a", "", 'return "crossed"')
  fields(order, OK, "string")
  t.eq(v.status, "ok", "string: a chunk through the door is answered")
  t.eq(v.result_type, "string", "string: typed")
  t.eq(v.chunkname, "=dcs-eval", "string: the default chunkname is echoed")
  t.eq(v.carrier, "a_do_script", "string: the carrier is the door")
  t.eq(v.via, "mission", "string: by way of mission")
  t.eq(body, "crossed", "string: the bytes verbatim")
  t.eq(#hops, 1, "string: net.dostring_in was called once")
  t.eq(hops[1].state, "mission", "string: into mission, not the state the request named")
  t.eq(#crossings, 1, "string: and the door was called once")

  -- Every type a chunk can return, and what the door was handed for it.
  local cases = {
    { "number", "return 42", "number", "42" },
    { "widened", "return 0.1 + 0.2", "number", "0.30000000000000004" },
    { "boolean", "return false", "boolean", "false" },
    { "nil", "return nil", "nil", "" },
    { "nothing", "local x = 1", "nil", "" },
    { "table", "return { 1, 2, 3 }", "table", "" },
    { "function", "return type", "function", "" },
    { "coroutine", "return coroutine.create(function() end)", "thread", "" },
  }
  for i, case in ipairs(cases) do
    local what = case[1]
    order, v, body = eval(E, frame, "1-b" .. i, "", case[2])
    fields(order, OK, what)
    t.eq(v.status, "ok", what .. ": answered")
    t.eq(v.result_type, case[3], what .. ": typed in the far state")
    t.eq(body, case[4], what .. ": printed there, or an empty body")
    local crossing = crossings[#crossings]
    t.eq(type(crossing.answered[2]), "string", what .. ": and what the door read in slot 2 was a string")
    t.eq(crossing.answered[2], "ok\n" .. case[3] .. "\n" .. case[4], what .. ": the three fields")
  end

  t.eq(entries(env, E.req), "", "values: every request is taken")
  t.eq(E.raised, 0, "values: nothing reached the guard")
  t.eq(E.unpublished, 0, "values: every reply was published")
end

--------------------------------------------------------------------------------
-- The door reads slot 2
--------------------------------------------------------------------------------

do
  local E, _, frame, mission, far, _, crossings = opened()

  local _, v, body = eval(E, frame, "2-a", "", 'return "lone"')
  t.eq(body, "lone", "shift: a lone value crosses")
  local crossing = crossings[#crossings]
  t.eq(crossing.returned.n, 2, "shift: because the far chunk returned it and one more")
  t.eq(crossing.returned[1], "ok\nstring\nlone", "shift: the payload first")
  t.eq(crossing.returned[2], 0, "shift: and the sacrificial 0 second")
  t.eq(crossing.answered.n, 2, "shift: the door answered two slots")
  t.eq(crossing.answered[1], nil, "shift: nil in slot 1")
  t.eq(crossing.answered[2], "ok\nstring\nlone", "shift: and the payload in slot 2, where it is read")

  -- A DCS build that corrected the shift puts the payload in slot 1 and
  -- the 0 in slot 2, and the door says so rather than reading nothing.
  install(mission, door(far, crossings, false))
  local order
  order, v, body = eval(E, frame, "2-b", "", 'return "lone"')
  fields(order, ERR, "unshifted")
  t.eq(v.status, "error", "unshifted: a door that does not shift is not an ok")
  t.eq(v.stage, "door", "unshifted: under stage door")
  t.eq(body, "slot 1 is string and slot 2 is number, where a_do_script's shift puts a nil in slot 1"
    .. " and the string payload in slot 2", "unshifted: naming both slots")

  t.eq(E.raised, 0, "shift: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The string-only backstop
--------------------------------------------------------------------------------

do
  local E, _, frame, mission, _, hops, _ = opened()

  -- A far chunk that escaped the wrapper: the door is handed a table in
  -- slot 2. It is refused by type, and not a key of it is read.
  local touched = {}
  local escaped = setmetatable({}, {
    __index = function(_, k)
      touched[#touched + 1] = tostring(k)
    end,
    __len = function()
      touched[#touched + 1] = "#"
      return 0
    end,
    __tostring = function()
      touched[#touched + 1] = "tostring"
      return "walked"
    end,
  })
  install(mission, function()
    return nil, escaped
  end)
  local order, v, body = eval(E, frame, "3-a", "", "return 1")
  fields(order, ERR, "table")
  t.eq(v.status, "error", "table: a table in slot 2 is not an ok")
  t.eq(v.stage, "door", "table: refused at the door")
  t.eq(body, "slot 1 is nil and slot 2 is table, where a_do_script's shift puts a nil in slot 1"
    .. " and the string payload in slot 2", "table: naming both slots")
  t.eq(#touched, 0, "table: without a key of it read: " .. table.concat(touched, ","))
  t.eq(hops[#hops].answered, "door\n\n" .. body, "table: the refusal is what crossed back from mission")

  install(mission, function()
    return nil, 42
  end)
  _, v, body = eval(E, frame, "3-b", "", "return 1")
  t.eq(v.stage, "door", "number: a number in slot 2 is refused too, not printed")
  t.check(body:find("^slot 1 is nil and slot 2 is number,"), "number: by type: " .. body)

  install(mission, function()
    return nil
  end)
  _, v, body = eval(E, frame, "3-c", "", "return 1")
  t.eq(v.stage, "door", "dropped: an empty slot 2, a lone value the shift dropped, is refused")
  t.check(body:find("^slot 1 is nil and slot 2 is nil,"), "dropped: by type: " .. body)

  -- A payload that is a string and not in the shape is the door's, not
  -- the state's.
  install(mission, function()
    return nil, "not the wrapper's"
  end)
  order, v, body = eval(E, frame, "3-d", "", "return 1")
  fields(order, ERR, "unshaped")
  t.eq(v.stage, "door", "unshaped: a string payload not in the shape is stage door")
  t.eq(body, "not the wrapper's", "unshaped: carried as the body")

  t.eq(E.raised, 0, "backstop: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The body crosses as a literal and an argument
--------------------------------------------------------------------------------

do
  local E, _, frame, _, _, hops, crossings = opened()

  local nul = 'return "a\0' .. '1b"'
  local _, v, body = eval(E, frame, "4-a", "chunkname: @x.lua\n", nul)
  t.eq(body, "a\0001b", "nul: a NUL before a digit decodes byte for byte through both hops")
  local chunk = hops[#hops].chunk
  local literal = string.format("%q", nul)
  t.eq(select(2, chunk:gsub("a\\0001b", "")), 1, "literal: the body's text appears once in the door chunk")
  t.check(chunk:find(", " .. literal .. ', "@x.lua")', 1, true),
    "literal: as a %q literal, the chunkname beside it")
  local crossing = crossings[#crossings]
  t.eq(crossing.args.n, 2, "argument: the door hands the far chunk two arguments")
  t.eq(crossing.args[1], nul, "argument: the body, byte for byte")
  t.eq(crossing.args[2], "@x.lua", "argument: and the chunkname")
  t.check(not crossing.source:find("a\0001b", 1, true), "far: the body is not in the far chunk's source")
  t.check(chunk:find(string.format("%q", crossing.source), 1, true), "far: which crosses into mission as a literal")

  local first = crossing.source
  _, v, body = eval(E, frame, "4-b", "chunkname: @y.lua\n", 'return "other"')
  t.eq(body, "other", "far: a second request crosses")
  t.eq(crossings[#crossings].source, first, "far: under the same far chunk, byte for byte")

  -- A body that is code in the door chunk's own terms stays a string.
  _, v, body = eval(E, frame, "4-c", "", '"), error("escaped") --')
  t.eq(v.status, "error", "quote: a body that closes a quote is the chunk's own compile error")
  t.eq(v.stage, "compile", "quote: compiled beyond the door, and nowhere before it")

  t.eq(E.raised, 0, "literal: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- Line numbers and globals beyond the door
--------------------------------------------------------------------------------

do
  local E, _, frame, mission, far, _, _ = opened()

  local order, v, body = eval(E, frame, "5-a", "chunkname: @mission.lua\n", at47('error("boom")'))
  fields(order, ERR, "47")
  t.eq(v.stage, "run", "47: a raise is stage run")
  t.eq(v.chunkname, "@mission.lua", "47: under the chunkname")
  t.eq(body, "mission.lua:47: boom", "47: on line 47, through both hops")

  order, v, body = eval(E, frame, "5-b", "chunkname: =far47\n", at47("return +"))
  fields(order, ERR, "compile")
  t.eq(v.stage, "compile", "compile: a body that does not compile is stage compile")
  t.check(body:find("^far47:47: "), "compile: naming the chunkname and the line: " .. body)

  local _
  _, v, body = eval(E, frame, "5-c", "", "door_seen = 7")
  t.eq(v.status, "ok", "globals: a chunk that sets a global runs")
  t.eq(rawget(far, "door_seen"), 7, "globals: and it landed in missionscripting")
  t.eq(rawget(mission, "door_seen"), nil, "globals: not in mission")
  _, v, body = eval(E, frame, "5-d", "", "return door_seen")
  t.eq(body, "7", "globals: the next chunk beyond the door reads it")
  _, v, body = eval(E, frame, "5-e", "", "return type(net)")
  t.eq(body, "table", "globals: missionscripting's net is there")
  _, v, body = eval(E, frame, "5-f", "", "return log")
  t.eq(v.stage, "run", "globals: mission's log is not")
  t.check(body:find("missionscripting%.log is not modelled"), "globals: the far model says so: " .. body)

  t.eq(E.raised, 0, "globals: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The ceiling is applied beyond the door
--------------------------------------------------------------------------------

do
  local E, _, frame, _, _, hops, crossings = opened()
  local ceiling = tonumber(published(E, "max_result_bytes"))
  t.eq(ceiling, 65536, "ceiling: the handshake publishes the figure")
  ---@cast ceiling integer

  local order, v, body = eval(E, frame, "6-a", "", "return string.rep('x', " .. ceiling .. ")")
  fields(order, OK, "at")
  t.eq(#body, ceiling, "at: a result of exactly the ceiling crosses whole")

  order, v, body = eval(E, frame, "6-b", "", "return string.rep('x', " .. (ceiling + 1) .. ")")
  fields(order, OVERSIZE, "over")
  t.eq(v.stage, "oversize", "over: one byte over is refused")
  t.eq(v.result_bytes, tostring(ceiling + 1), "over: result_bytes is the length refused")
  t.eq(crossings[#crossings].answered[2], "oversize\nresult\n" .. (ceiling + 1),
    "over: what the door read was the refusal, so the result never crossed it")
  t.eq(hops[#hops].answered, "oversize\nresult\n" .. (ceiling + 1), "over: nor the first hop")

  t.eq(E.raised, 0, "ceiling: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The door is shut with no mission
--------------------------------------------------------------------------------

do
  local E, env, _, frame = loaded("hook")
  local hops, crossings = {}, {}
  local mission = t.state("mission", {})
  local far = t.state("missionscripting", {})
  carrier(env, { mission = mission }, hops)

  local order, v, body = eval(E, frame, "7-a", "", 'return "crossed"')
  fields(order, HEAD, "shut")
  t.eq(v.status, "door-shut", "shut: no mission loaded is door-shut, a status of its own")
  t.eq(v.carrier, "a_do_script", "shut: through the door")
  t.eq(body, "no mission is loaded: a_do_script is nil in the mission state, and the door opens only"
    .. " with a mission loaded", "shut: saying why")
  t.eq(hops[1].answered, "door-shut\n\n" .. body, "shut: answered from inside mission")

  -- Loaded, the same request crosses; unloaded again, it does not. The
  -- door is read when it is used, never kept from a previous crossing.
  install(mission, door(far, crossings))
  order, v, body = eval(E, frame, "7-b", "", 'return "crossed"')
  fields(order, OK, "loaded")
  t.eq(body, "crossed", "loaded: a mission loaded opens the door")
  install(mission, nil)
  order, v, body = eval(E, frame, "7-c", "", 'return "crossed"')
  fields(order, HEAD, "unloaded")
  t.eq(v.status, "door-shut", "unloaded: and unloading it shuts the door again")
  t.eq(#crossings, 1, "unloaded: the door was called for the loaded mission alone")

  install(mission, true)
  _, v, body = eval(E, frame, "7-d", "", "return 1")
  t.eq(v.status, "door-shut", "not a function: a door that is not a function is shut")
  t.check(body:find("a_do_script is boolean", 1, true), "not a function: naming its type: " .. body)

  t.eq(E.raised, 0, "shut: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- A door that fails
--------------------------------------------------------------------------------

do
  local E, _, frame, mission, _, hops, _ = opened()

  install(mission, function()
    error("the door jammed", 0)
  end)
  local order, v, body = eval(E, frame, "8-a", "", "return 1")
  fields(order, ERR, "raised")
  t.eq(v.stage, "door", "raised: a_do_script raising is stage door")
  t.eq(body, "a_do_script raised: the door jammed", "raised: with the message")

  install(mission, function()
    error({}, 0)
  end)
  local _
  _, v, body = eval(E, frame, "8-b", "", "return 1")
  t.eq(body, "a_do_script raised: (error object is a table value)", "raised table: named by type")

  -- A door that runs the far chunk and hands it nothing.
  local far = t.state("missionscripting", {})
  install(mission, function(source)
    local fn = assert(loadstring(source, "=far"))
    setfenv(fn, far)
    return nil, (fn())
  end)
  order, v, body = eval(E, frame, "8-c", "", "return 1")
  fields(order, ERR, "no arguments")
  t.eq(v.stage, "door", "no arguments: a far chunk handed no body says so")
  t.eq(body, "the far chunk was handed a nil body and a nil chunkname, where a_do_script passes its"
    .. " arguments on as strings", "no arguments: naming what it was handed")

  t.eq(E.raised, 0, "failures: nothing reached the guard")
  t.eq(#hops, 3, "failures: each crossed the first hop")
end

do
  -- The door chunk's own raise, the executor's failure: a mission with no
  -- `rawget` under it.
  local E, env, _, frame = loaded("hook")
  local hops = {}
  local broken = t.state("mission", { mission_loaded = true })
  broken.rawget = nil
  carrier(env, { mission = broken }, hops)
  local order, v, body = eval(E, frame, "8-d", "", "return 1")
  fields(order, ERR, "door raise")
  t.eq(v.stage, "bridge", "door raise: the door chunk's own raise is stage bridge")
  t.check(body:find("rawget", 1, true), "door raise: with the message: " .. body)
  t.eq(E.raised, 0, "door raise: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The first hop's answers, and the export host
--------------------------------------------------------------------------------

do
  local E, env, _, frame = loaded("hook")
  local _

  -- The model's own stub evaluates nothing and answers empty.
  local order, v, body = eval(E, frame, "9-a", "", "return 1")
  fields(order, ERR, "stub")
  t.eq(v.status, "error", "stub: the model's empty answer is not an ok")
  t.eq(v.stage, "door", "stub: and is not the door's answer")
  t.eq(body, "", "stub: carried as the body")

  local asked = {}
  answering(env, function(state)
    asked[#asked + 1] = state
    return nil
  end)
  order, v, body = eval(E, frame, "9-b", "", "return 1")
  fields(order, HEAD, "refused")
  t.eq(v.status, "refused", "refused: nil from the first hop is refused")
  t.check(body:find("^net%.dostring_in returned nil for mission: "), "refused: naming mission: " .. body)
  t.eq(asked[1], "mission", "refused: the first hop was asked for mission")

  answering(env, function()
    return "Invalid state name"
  end)
  order, v, body = eval(E, frame, "9-c", "", "return 1")
  fields(order, HEAD, "invalid")
  t.eq(v.status, "invalid-state", "invalid: the literal from the first hop is invalid-state")

  answering(env, function()
    return {}
  end)
  order, v, body = eval(E, frame, "9-d", "", "return 1")
  fields(order, ERR, "not a string")
  t.eq(v.stage, "dostring_in", "not a string: the first hop answering a table is its own stage")

  -- A status only the door sends is read only through the door.
  answering(env, function()
    return "door-shut\n\nfrom gui"
  end)
  local plain = "op: eval\nfor: " .. E.stamp .. "\nstate: gui\n\nreturn 1"
  request(E, "9-e.req", plain)
  frame()
  order, v, body = read(E, "9-e")
  t.eq(v.status, "error", "gui: door-shut from another state is not the door's status")
  t.eq(v.stage, "dostring_in", "gui: it is an unshaped answer there")
  t.eq(v.carrier, nil, "gui: and a reply from gui carries no carrier")

  env.net = nil
  order, v, body = eval(E, frame, "9-f", "", "return 1")
  fields(order, HEAD, "no net")
  t.eq(v.status, "unsupported", "no net: a host without net.dostring_in has no door")
  t.eq(body, "no net.dostring_in on this host", "no net: saying so")

  t.eq(entries(env, E.req), "", "first hop: every request is taken")
  t.eq(E.raised, 0, "first hop: nothing reached the guard")
end

do
  local E, _, _, frame = loaded("export")
  request(E, "10-a.req", "op: eval\nfor: " .. E.stamp .. "\nstate: missionscripting\n\nreturn 1")
  frame()
  local order, v, body = read(E, "10-a")
  fields(order, PLAIN, "export")
  t.eq(v.status, "unsupported", "export: the export host has no door")
  t.eq(body, "missionscripting is not a state this host serves", "export: saying so")
  t.eq(E.raised, 0, "export: nothing reached the guard")
end

