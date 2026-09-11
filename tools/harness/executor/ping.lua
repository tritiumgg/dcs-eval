-- The tick and the `ping` op: the frame callback listing the request
-- directory, admitting what it finds and dispatching it, driven over a
-- sandbox by calling the callback the load registered, the way
-- `executor/request` drives `admit`.
--
-- What is proved. A `ping` planted under `req` is answered on the next
-- frame under `res`, the request gone, and the reply is one envelope whose
-- headers are exactly `status`, `protocol`, `host`, `stamp`, `phase`, `id`,
-- `tick`, `states`, `last_callback`, `callbacks`, in that order, with
-- `pong` as the body. `tick` counts frames from the load and stamps the
-- reply with the frame that answered it; `states` is what the handshake
-- declares; `last_callback` is the last callback other than the frame to
-- fire, as `<name>@<tick>`, and `callbacks` every name seen in the order
-- first seen, the frame never among them, both empty until one has fired.
-- Several requests in one frame are answered in name order and share the
-- tick; a `.req.tmp` and a file under another name are left alone; a frame
-- with nothing to answer touches no file. An unknown op is `bad-request`
-- naming it, `eval` is `unsupported` until it is served, and neither runs
-- anything, because `loadstring` and `net.dostring_in` are replaced with
-- ones that fail a check. An op that raises is answered `error` under
-- `stage: bridge` with the message, the next request in the same frame is
-- answered still, and no raise reaches the guard. A request read and not
-- removable is answered `error` once and never opened again while it stays,
-- and its name is answered afresh once it has gone and come back. A reply
-- that cannot be published is counted on the namespace with its reason,
-- noted once in `dcs.log`, and stops nothing, and is counted still when
-- the request it answers is one that could not be removed, which is held
-- as any other. A `for` that is not the stamp
-- is answered here, because the fence is not built. The export host ticks
-- on the callback after the frame and not the one before it, and answers
-- under its own phase and its one state.
--
-- The mutations this suite exists to catch. Drop `status` from the reply
-- and the header-order check reads the list one short, naming what is
-- missing. Leave `tick` unincremented and the first reply reads `tick: 0`.
-- Record the frame callback and `callbacks` names it. Match `.req` as a
-- prefix or a substring and the `.req.tmp` planted beside a request is
-- taken. Drop the per-request pcall and the raising op stops the frame
-- with the ping after it unanswered. Forget to skip a held name and the
-- second frame opens it again. Tick on `LuaExportBeforeNextFrame` and the
-- export case reads a reply where it wants the request still there.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The reply's headers, in order. The suite's own copy, kept apart from the
-- executor's on purpose. HEAD is what every reply carries; a `ping` adds
-- the three after it.
local HEAD = { "status", "protocol", "host", "stamp", "phase", "id", "tick" }
local PING = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "states", "last_callback", "callbacks" }

local HOOK_STATES = "hook:carrier=local,returns=any,needs=always"
  .. " gui:carrier=dostring_in,returns=string,needs=menu"
  .. " scripting:carrier=dostring_in,returns=string,needs=menu"
  .. " mission:carrier=dostring_in,returns=string,needs=menu"
  .. " config:carrier=dostring_in,returns=string,needs=menu"
  .. " export:carrier=dostring_in,returns=string,needs=slot"
  .. " missionscripting:carrier=a_do_script,returns=string,needs=mission"
local EXPORT_STATES = "export:carrier=local,returns=any,needs=always"

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

-- A host whose directories are under a fresh sandbox, and the sandbox.
local function sandboxed(host)
  local box = t.sandbox()
  host = host or {}
  if host.writedir == nil then
    host.writedir = box .. SAVED
  end
  if host.tempdir == nil then
    host.tempdir = box .. TEMP
  end
  return host, box
end

-- A state over a sandbox with spies on the calls a take and a publish
-- make, installed before the load so nothing the executor captured can be
-- the original, and the log emptied after it. `loadstring` and
-- `net.dostring_in` are replaced with ones that fail a check: no op here
-- runs anything. Returns the namespace, the state, the sandbox, the log and
-- the host, whose `callbacks` is what the hook load registered.
local function spied(state, host)
  local box
  host, box = sandboxed(host)
  local env = t.state(state, host)
  local log = {}
  local open, remove, rename = env.io.open, env.os.remove, env.os.rename
  env.io.open = function(path, m)
    log[#log + 1] = { "open", m, path }
    return open(path, m)
  end
  env.os.remove = function(path)
    log[#log + 1] = { "remove", path }
    return remove(path)
  end
  env.os.rename = function(from, to)
    log[#log + 1] = { "rename", from, to }
    return rename(from, to)
  end
  env.loadstring = function()
    t.check(false, "the executor compiled something: nothing under this suite may run")
  end
  if state ~= "export" then
    env.net.dostring_in = function()
      t.check(false, "the executor sent something to a state: nothing under this suite may run")
    end
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  for i = #log, 1, -1 do
    log[i] = nil
  end
  return E, env, box, log, host
end

local function clear(log)
  for i = #log, 1, -1 do
    log[i] = nil
  end
end

-- One line per call in an operation log.
local function ops(log)
  local lines = {}
  for i, entry in ipairs(log) do
    lines[i] = table.concat(entry, " ")
  end
  return table.concat(lines, "\n")
end

-- One request under `E.req`, written through the runner's own `io` so the
-- log holds nothing of it. Returns the path.
local function request(E, name, content)
  local path = E.req .. "\\" .. name
  local fh = assert(io.open(path, "wb"))
  fh:write(content)
  fh:close()
  return path
end

-- A `ping` for the session, under `name`.
local function ping(E, name)
  return request(E, name, "op: ping\nfor: " .. E.stamp .. "\n\n")
end

-- The one sequence a take of `path` makes: opened for bytes, then removed.
local function taken(path)
  return "open rb " .. path .. "\nremove " .. path
end

-- The one sequence a publish of `path` makes.
local function published(path)
  return "open wb " .. path .. ".tmp\nremove " .. path .. "\nrename " .. path .. ".tmp " .. path
end

-- A request taken and its reply published, and nothing else.
local function answered(E, id)
  return taken(E.req .. "\\" .. id .. ".req") .. "\n" .. published(E.res .. "\\" .. id .. ".res")
end

-- A reply read back with the suite's own reader, not the executor's
-- parser: the names in the order written, the values by name, and the
-- body. Every header line must read `name: value`, and a reply that is not
-- there stops the suite naming it.
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

--------------------------------------------------------------------------------
-- A ping, answered on the frame
--------------------------------------------------------------------------------

do
  local E, env, _, log, host = spied("hook")
  t.eq(type(E), "table", "the namespace is published")
  t.eq(type(E.ops), "table", "and carries the op table")
  t.eq(type(E.ops.ping), "function", "with ping in it")
  t.eq(E.tick, 0, "the tick starts at 0")
  local frame = host.callbacks.onSimulationFrame

  local path = ping(E, "0000000001-abcd.req")
  frame()
  t.eq(E.tick, 1, "one frame is one tick")
  t.eq(entries(env, E.req), "", "the request is gone")
  t.eq(entries(env, E.res), "0000000001-abcd.res", "and the reply is under res, no .tmp")
  t.eq(ops(log), taken(path) .. "\n" .. published(E.res .. "\\0000000001-abcd.res"),
    "the request was taken, then the reply published, and nothing else was touched")
  local order, v, body = read(E, "0000000001-abcd")
  fields(order, PING, "ping")
  t.eq(v.status, "ok", "ping: ok")
  t.eq(v.protocol, "2", "ping: protocol 2")
  t.eq(v.host, "hook", "ping: the host")
  t.eq(v.stamp, E.stamp, "ping: the stamp")
  t.eq(v.phase, "menu", "ping: the phase")
  t.eq(v.id, "0000000001-abcd", "ping: the id")
  t.eq(v.tick, "1", "ping: the tick it was answered on")
  t.eq(v.states, HOOK_STATES, "ping: the states the handshake declares")
  t.eq(v.last_callback, "", "ping: no rare callback has fired, so last_callback is empty")
  t.eq(v.callbacks, "", "ping: and so is callbacks")
  t.eq(body, "pong", "ping: the body is pong")
  t.eq(E.raised, 0, "ping: nothing raised")

  -- The record: rare callbacks name themselves with the tick they fired
  -- at, once each in the list, and the frame never.
  host.callbacks.onShowMainInterface()
  frame()
  frame()
  host.callbacks.onSimulationStart()
  host.callbacks.onShowMainInterface()
  ping(E, "2-a.req")
  frame()
  order, v, body = read(E, "2-a")
  fields(order, PING, "record")
  t.eq(v.tick, "4", "record: the fourth frame")
  t.eq(v.phase, "sim", "record: the phase moved with the callback")
  t.eq(v.last_callback, "onShowMainInterface@3", "record: the last rare callback, at the tick it fired")
  t.eq(v.callbacks, "onShowMainInterface,onSimulationStart", "record: every name seen, once, in the order first seen")
  t.eq(body, "pong", "record: pong still")

  -- A ping with a body is a ping.
  request(E, "2-b.req", "op: ping\nfor: " .. E.stamp .. "\n\nignored")
  frame()
  order, v, body = read(E, "2-b")
  t.eq(v.status, "ok", "body: a ping with a body is answered")
  t.eq(body, "pong", "body: and the body is pong, not the request's")

  -- A reply made outside the frame carries the tick as it stands.
  t.eq(E.reply("2-c", "ok", {}, ""), true, "outside: a reply is published")
  order, v = read(E, "2-c")
  fields(order, HEAD, "outside")
  t.eq(v.tick, "5", "outside: the tick as it stands")
end

--------------------------------------------------------------------------------
-- Several in one frame, in name order
--------------------------------------------------------------------------------

do
  local E, env, _, log, host = spied("hook")
  local frame = host.callbacks.onSimulationFrame
  ping(E, "3-c.req")
  ping(E, "3-a.req")
  ping(E, "3-b.req")
  request(E, "3-d.req.tmp", "op: ping\nfor: " .. E.stamp .. "\n\n")
  request(E, "notes.txt", "op: ping\nfor: " .. E.stamp .. "\n\n")
  frame()
  t.eq(ops(log), answered(E, "3-a") .. "\n" .. answered(E, "3-b") .. "\n" .. answered(E, "3-c"),
    "order: each is taken and answered in name order, whatever order they were planted in")
  t.eq(entries(env, E.req), "3-d.req.tmp notes.txt", "order: the .tmp and the file under another name are left alone")
  t.eq(entries(env, E.res), "3-a.res 3-b.res 3-c.res", "order: the three replies")
  for _, id in ipairs({ "3-a", "3-b", "3-c" }) do
    local _, v = read(E, id)
    t.eq(v.tick, "1", "order: " .. id .. " shares the tick")
    t.eq(v.status, "ok", "order: " .. id .. " is ok")
  end

  clear(log)
  frame()
  t.eq(E.tick, 2, "quiet: the tick advances")
  t.eq(#log, 0, "quiet: a frame with nothing to answer opens, removes and renames nothing")
  t.eq(entries(env, E.req), "3-d.req.tmp notes.txt", "quiet: and still leaves the two alone")
end

--------------------------------------------------------------------------------
-- An op the table lacks
--------------------------------------------------------------------------------

do
  local E, env, _, _, host = spied("hook")
  request(E, "4-a.req", "op: nope\nfor: " .. E.stamp .. "\n\n")
  request(E, "4-b.req", "op: eval\nfor: " .. E.stamp .. "\n\nreturn 1")
  request(E, "4-c.req", "op: Ping\nfor: " .. E.stamp .. "\n\n")
  host.callbacks.onSimulationFrame()
  t.eq(entries(env, E.req), "", "unknown: every request is taken")
  local order, v, body = read(E, "4-a")
  fields(order, HEAD, "unknown")
  t.eq(v.status, "bad-request", "unknown: an op the table lacks is bad-request")
  t.eq(body, "unknown op: nope", "unknown: naming it")
  t.eq(v.tick, "1", "unknown: with the tick")
  order, v, body = read(E, "4-b")
  fields(order, HEAD, "eval")
  t.eq(v.status, "unsupported", "eval: declared and not served is unsupported, not unknown")
  t.check(body:find("eval", 1, true) and body:find("not yet served", 1, true), "eval: the body says so: " .. body)
  _, v, body = read(E, "4-c")
  t.eq(v.status, "bad-request", "case: an op is matched by its spelling")
  t.eq(body, "unknown op: Ping", "case: Ping is not ping")
  t.eq(E.raised, 0, "unknown: nothing raised")
end

--------------------------------------------------------------------------------
-- An op that raises
--------------------------------------------------------------------------------

do
  local E, env, _, _, host = spied("hook")
  E.ops.boom = function()
    error("boom", 0)
  end
  E.ops.table = function()
    error({ what = "a table" })
  end
  request(E, "5-a.req", "op: boom\nfor: " .. E.stamp .. "\n\n")
  ping(E, "5-b.req")
  request(E, "5-c.req", "op: table\nfor: " .. E.stamp .. "\n\n")
  host.callbacks.onSimulationFrame()
  t.eq(entries(env, E.res), "5-a.res 5-b.res 5-c.res", "raise: every request is answered")
  local order, v, body = read(E, "5-a")
  fields(order, { "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage" }, "raise")
  t.eq(v.status, "error", "raise: an op that raises is error")
  t.eq(v.stage, "bridge", "raise: under stage bridge")
  t.eq(body, "boom", "raise: with the message as the body")
  _, v, body = read(E, "5-b")
  t.eq(v.status, "ok", "raise: the ping after it is answered still")
  t.eq(body, "pong", "raise: with pong")
  _, v, body = read(E, "5-c")
  t.eq(v.status, "error", "raise: a raise that is not a string is error too")
  t.check(body:find("^table: "), "raise: with the value's tostring as the body: " .. body)
  t.eq(E.raised, 0, "raise: the guard saw no raise, the tick caught it")
  t.eq(E.last_raise, nil, "raise: and recorded none")
end

--------------------------------------------------------------------------------
-- A request read and not removable
--------------------------------------------------------------------------------

do
  local E, env, _, log, host = spied("hook")
  local frame = host.callbacks.onSimulationFrame
  local path = ping(E, "6-a.req")
  local holder = assert(io.open(path, "rb"))
  frame()
  t.eq(entries(env, E.req), "6-a.req", "held: the request stays")
  local order, v, body = read(E, "6-a")
  fields(order, { "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage" }, "held")
  t.eq(v.status, "error", "held: answered error")
  t.eq(v.stage, "bridge", "held: under stage bridge")
  t.check(body:find("could not be removed", 1, true), "held: saying why: " .. body)
  t.eq(E.unpublished, 0, "held: the reply was published, so nothing is counted lost")
  assert(os.remove(E.res .. "\\6-a.res"))

  clear(log)
  frame()
  frame()
  t.eq(#log, 0, "held: while it stays it is never opened again")
  t.eq(entries(env, E.res), "", "held: and never answered again")
  t.eq(E.tick, 3, "held: the frames went on")

  holder:close()
  assert(os.remove(path))
  frame()
  t.eq(#log, 0, "held: once it is gone there is nothing to do")

  path = ping(E, "6-a.req")
  frame()
  t.eq(ops(log), taken(path) .. "\n" .. published(E.res .. "\\6-a.res"), "held: the name come back is a new request")
  _, v, body = read(E, "6-a")
  t.eq(v.status, "ok", "held: answered ok")
  t.eq(v.tick, "5", "held: on the fifth frame")
end

--------------------------------------------------------------------------------
-- A reply that cannot be published
--------------------------------------------------------------------------------

do
  local E, env, box, _, host = spied("hook")
  E.res = box .. "\\nowhere"
  ping(E, "7-a.req")
  request(E, "7-b.req", "op: eval\n\nx")
  host.callbacks.onSimulationFrame()
  t.eq(entries(env, E.req), "", "lost: the requests are gone all the same")
  t.eq(E.unpublished, 2, "lost: both replies are counted")
  t.check(type(E.last_unpublished) == "string" and E.last_unpublished:find("7-b", 1, true),
    "lost: the latest reason names its request: " .. tostring(E.last_unpublished))
  t.eq(host.log and #host.log, 2, "lost: one dcs.log line each")
  t.eq(host.log[1].subsystem, NAME, "lost: under the file's name")
  t.eq(host.log[1].level, env.log.WARNING, "lost: a warning")
  t.check(host.log[1].message:find("7-a", 1, true), "lost: the first names the ping: " .. host.log[1].message)
  t.check(host.log[2].message:find("7-b", 1, true), "lost: the second names the refusal: " .. host.log[2].message)
  t.eq(E.raised, 0, "lost: nothing raised")
  t.eq(E.tick, 1, "lost: the frame completed")
end

-- Both at once: a request that cannot be removed whose error reply cannot
-- be published is held, and the lost reply is counted all the same.
do
  local E, env, box, log, host = spied("hook")
  local path = ping(E, "7-c.req")
  local holder = assert(io.open(path, "rb"))
  local res = E.res
  E.res = box .. "\\nowhere"
  host.callbacks.onSimulationFrame()
  t.eq(entries(env, E.req), "7-c.req", "held and lost: the request stays")
  t.eq(E.unpublished, 1, "held and lost: the reply that was not published is counted")
  t.check(type(E.last_unpublished) == "string" and E.last_unpublished:find("error reply to 7-c", 1, true),
    "held and lost: with the reason naming it: " .. tostring(E.last_unpublished))
  t.eq(host.log and #host.log, 1, "held and lost: one dcs.log line")
  E.res = res
  clear(log)
  host.callbacks.onSimulationFrame()
  t.eq(#log, 0, "held and lost: the request is held and not opened again")
  t.eq(E.unpublished, 1, "held and lost: and nothing more is counted")
  holder:close()
end

--------------------------------------------------------------------------------
-- The fence, not yet built
--------------------------------------------------------------------------------

do
  local E, _, _, _, host = spied("hook")
  request(E, "8-a.req", "op: ping\nfor: not-the-stamp\n\n")
  host.callbacks.onSimulationFrame()
  local _, v, body = read(E, "8-a")
  t.eq(v.status, "ok", "foreign: a for that is not the stamp is answered here; the fence, when it comes, answers stale-session")
  t.eq(body, "pong", "foreign: with pong")
end

--------------------------------------------------------------------------------
-- The export host
--------------------------------------------------------------------------------

do
  local E, env, _, log = spied("export")
  t.eq(type(E.ops.ping), "function", "export: ping is in the table")
  ping(E, "9-a.req")
  rawget(env, "LuaExportBeforeNextFrame")()
  t.eq(E.tick, 0, "export: the callback before the frame is not the tick")
  t.eq(entries(env, E.req), "9-a.req", "export: and lists nothing")
  t.eq(#log, 0, "export: nor opens anything")
  rawget(env, "LuaExportStart")()
  t.eq(E.phase, "sim", "export: the start moved the phase")
  rawget(env, "LuaExportAfterNextFrame")()
  t.eq(E.tick, 1, "export: the callback after the frame is the tick")
  t.eq(entries(env, E.req), "", "export: the request is gone")
  local order, v, body = read(E, "9-a")
  fields(order, PING, "export")
  t.eq(v.status, "ok", "export: ok")
  t.eq(v.host, "export", "export: the host")
  t.eq(v.phase, "sim", "export: the phase")
  t.eq(v.tick, "1", "export: the tick")
  t.eq(v.states, EXPORT_STATES, "export: the one state")
  t.eq(v.last_callback, "LuaExportStart@0", "export: the start fired before any frame")
  t.eq(v.callbacks, "LuaExportStart", "export: and is the one name seen")
  t.eq(body, "pong", "export: pong")
  rawget(env, "LuaExportStop")()
  ping(E, "9-b.req")
  rawget(env, "LuaExportAfterNextFrame")()
  _, v = read(E, "9-b")
  t.eq(v.phase, "stopped", "export: after the stop the phase is stopped")
  t.eq(v.last_callback, "LuaExportStop@1", "export: and the stop is the last callback")
  t.eq(v.callbacks, "LuaExportStart,LuaExportStop", "export: the frame callbacks are never in the list")
  t.eq(E.raised, 0, "export: nothing raised")
end
