-- The tick's budget and the cost every reply carries: the frame callback
-- driven over a sandbox, as `executor/ping` drives it, with the model's
-- clock moved by the suite. The model's clock stands still unless moved, so
-- every millisecond spent here is one the suite spent on purpose: an op the
-- suite hangs on the namespace, `spend`, moves it by its `ms` header, and a
-- wrapped `lfs.dir` or `io.open` moves it where a case wants the listing or
-- the take to cost something.
--
-- What is proved. Every reply carries `cpu_ms` directly after `tick`, as
-- milliseconds to three places: a `ping`, an `eval` that ran, a refusal
-- `admit` made, an op that raised, and a reply framed outside any tick,
-- which handled nothing and reads `0.000`. Eight `eval`s planted out of
-- order, W deep, are taken in id order on one frame and share its tick.
-- The cost is each request's own, not the tick's so far, and counts the
-- take. A tick stops taking requests once it has spent `TICK_BUDGET_MS`,
-- read before each request after the first: the requests it did not reach
-- are not opened, stay on the disk, and are answered in order on the next
-- frame. Spending exactly the budget stops the tick. The listing is spent
-- from the budget too. The first request is answered whatever the listing
-- or it costs, so one over the budget alone serialises one reply a frame.
-- The export host holds its tick, on the callback after the frame, to the
-- same budget. A state with no `os.clock`, or one that is not a function,
-- stops the load in both hosts, before anything is made.
--
-- The mutations this suite exists to catch. Drop `cpu_ms` from the reply
-- and the presence check names it missing. Compare with `>` and the tick
-- spending exactly the budget takes the third request. Check the budget
-- before the first request too and a tick whose listing spent the budget
-- answers nothing, and would answer nothing on every frame after. Start the clock after the listing and the costly listing lets
-- every request through. Read `began` after the take and the take's two
-- milliseconds go uncounted. Charge from the start of the tick and the
-- third request of the tick reads nine milliseconds, not three. Drop the
-- load-time clock test and the load goes on without one.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The figure the executor publishes, the suite's own copy.
local BUDGET_MS = 8

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
  host.writedir = host.writedir or box .. SAVED
  host.tempdir = host.tempdir or box .. TEMP
  return host, box
end

-- A loaded state over a sandbox with a spy on every `io.open`, installed
-- before the load, and the `spend` op on the namespace. The log is the
-- paths opened, in order, emptied after the load. Returns the namespace,
-- the state, the log and the host.
local function loaded(state)
  local host = sandboxed()
  host.clock = 0
  local env = t.state(state, host)
  local log = {}
  local open = env.io.open
  env.io.open = function(path, m)
    log[#log + 1] = m .. " " .. path
    return open(path, m)
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  -- This suite drives the armed path, never the wake: a load is asleep, so
  -- the arm the arm file would do is done here by hand.
  E.armed = true
  t.eq(type(E), "table", state .. ": the namespace is published")
  E.ops.spend = function(req)
    host.clock = host.clock + tonumber(req.headers.ms) / 1000
    return E.reply(req.id, "ok", nil, "spent " .. req.headers.ms)
  end
  for i = #log, 1, -1 do
    log[i] = nil
  end
  return E, env, log, host
end

-- One request under `E.req`, written through the runner's own `io` so the
-- log holds nothing of it.
local function request(E, name, content)
  local fh = assert(io.open(E.req .. "\\" .. name, "wb"))
  fh:write(content)
  fh:close()
end

local function ping(E, id)
  request(E, id .. ".req", "op: ping\nfor: " .. E.stamp .. "\n\n")
end

local function spend(E, id, ms)
  request(E, id .. ".req", "op: spend\nfor: " .. E.stamp .. "\nms: " .. ms .. "\n\n")
end

-- The ids of the requests opened for reading, in the order they were.
local function taken(log)
  local ids = {}
  for _, line in ipairs(log) do
    local id = line:match("^rb .*\\([^\\]+)%.req$")
    if id then
      ids[#ids + 1] = id
    end
  end
  return table.concat(ids, " ")
end

local function clear(log)
  for i = #log, 1, -1 do
    log[i] = nil
  end
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

-- The presence check: `cpu_ms` is on the reply, directly after `tick`,
-- milliseconds to three places. Returns the value.
local function costed(E, id, what)
  local order, v = read(E, id)
  local at
  for i, name in ipairs(order) do
    if name == "cpu_ms" then
      at = i
    end
  end
  t.check(at, what .. ": the reply carries cpu_ms, but its headers are " .. table.concat(order, " "))
  t.eq(order[at - 1], "tick", what .. ": directly after tick")
  t.check(v.cpu_ms:find("^%d+%.%d%d%d$"), what .. ": milliseconds to three places: " .. v.cpu_ms)
  return v.cpu_ms, v
end

--------------------------------------------------------------------------------
-- Every reply carries its cost
--------------------------------------------------------------------------------

do
  local E, _, _, host = loaded("hook")
  E.ops.boom = function()
    error("boom", 0)
  end
  ping(E, "1-a")
  request(E, "1-b.req", "op: eval\nfor: " .. E.stamp .. "\nstate: hook\n\nreturn 42")
  request(E, "1-c.req", "op: ping\n\n")
  request(E, "1-d.req", "op: boom\nfor: " .. E.stamp .. "\n\n")
  host.callbacks.onSimulationFrame()
  local cost, v = costed(E, "1-a", "ping")
  t.eq(cost, "0.000", "ping: a request that spent nothing costs nothing")
  t.eq(v.status, "ok", "ping: answered")
  cost, v = costed(E, "1-b", "eval")
  t.eq(cost, "0.000", "eval: the cost of the chunk")
  t.eq(v.result_type, "number", "eval: the chunk ran")
  cost, v = costed(E, "1-c", "refusal")
  t.eq(v.status, "bad-request", "refusal: admit refused it")
  t.eq(cost, "0.000", "refusal: and it carries the cost all the same")
  cost, v = costed(E, "1-d", "raise")
  t.eq(v.stage, "bridge", "raise: the op raised")
  t.eq(cost, "0.000", "raise: and the reply to it carries the cost")

  host.clock = 5
  t.eq(E.reply("1-e", "ok", nil, ""), true, "outside: a reply framed outside a tick is published")
  t.eq(costed(E, "1-e", "outside"), "0.000", "outside: it handled nothing, wherever the clock stands")
end

--------------------------------------------------------------------------------
-- W deep: eight on one frame, in id order
--------------------------------------------------------------------------------

do
  local E, env, log, host = loaded("hook")
  local ids = {}
  for n = 8, 1, -1 do
    local id = string.format("%010d-w", n)
    request(E, id .. ".req", "op: eval\nfor: " .. E.stamp .. "\nstate: hook\n\nreturn " .. n)
    table.insert(ids, 1, id)
  end
  host.callbacks.onSimulationFrame()
  t.eq(taken(log), table.concat(ids, " "), "W deep: taken in id order, whatever order they were planted in")
  t.eq(entries(env, E.req), "", "W deep: every one was taken on the one frame")
  for n, id in ipairs(ids) do
    local _, v, body = read(E, id)
    t.eq(v.tick, "1", "W deep: " .. id .. " shares the tick")
    t.eq(body, tostring(n), "W deep: " .. id .. " answers its own chunk")
    costed(E, id, "W deep: " .. id)
  end
end

--------------------------------------------------------------------------------
-- Past the budget: the rest wait for the next frame
--------------------------------------------------------------------------------

do
  local E, env, log, host = loaded("hook")
  local frame = host.callbacks.onSimulationFrame
  for i = 1, 5 do
    spend(E, "2-" .. i, 3)
  end
  frame()
  t.eq(taken(log), "2-1 2-2 2-3", "past: three at 3 ms reach 9, and the tick takes no fourth")
  t.eq(entries(env, E.req), "2-4.req 2-5.req", "past: the two it did not reach stay on the disk")
  t.eq(entries(env, E.res), "2-1.res 2-2.res 2-3.res", "past: and have no reply yet")
  clear(log)
  frame()
  t.eq(taken(log), "2-4 2-5", "past: the next frame answers them, in order")
  t.eq(entries(env, E.req), "", "past: and nothing is left")
  for i = 1, 5 do
    local cost, v = costed(E, "2-" .. i, "past: 2-" .. i)
    t.eq(v.tick, i <= 3 and "1" or "2", "past: 2-" .. i .. " on its frame")
    t.eq(cost, "3.000", "past: 2-" .. i .. " is charged its own 3 ms, not the tick's")
  end
end

--------------------------------------------------------------------------------
-- Exactly the budget stops the tick
--------------------------------------------------------------------------------

do
  local E, env, log, host = loaded("hook")
  spend(E, "3-a", BUDGET_MS / 2)
  spend(E, "3-b", BUDGET_MS / 2)
  ping(E, "3-c")
  host.callbacks.onSimulationFrame()
  t.eq(host.clock * 1000, BUDGET_MS, "at: the two spent exactly the budget")
  t.eq(taken(log), "3-a 3-b", "at: a tick that has spent exactly its budget takes nothing more")
  t.eq(entries(env, E.req), "3-c.req", "at: the ping waits")
  host.callbacks.onSimulationFrame()
  local _, v = read(E, "3-c")
  t.eq(v.tick, "2", "at: and is answered on the next frame")
end

--------------------------------------------------------------------------------
-- One over the budget alone
--------------------------------------------------------------------------------

do
  local E, env, log, host = loaded("hook")
  local frame = host.callbacks.onSimulationFrame
  spend(E, "4-a", 20)
  spend(E, "4-b", 20)
  ping(E, "4-c")
  frame()
  t.eq(taken(log), "4-a", "alone: the first request is taken, though it spends the budget twice over")
  frame()
  frame()
  t.eq(entries(env, E.req), "", "alone: one a frame, until none is left")
  for id, tick in pairs({ ["4-a"] = "1", ["4-b"] = "2", ["4-c"] = "3" }) do
    local _, v = read(E, id)
    t.eq(v.tick, tick, "alone: " .. id .. " on frame " .. tick)
  end
  t.eq(costed(E, "4-a", "alone"), "20.000", "alone: charged what it spent")
end

--------------------------------------------------------------------------------
-- The listing is spent from the budget
--------------------------------------------------------------------------------

do
  local E, env, log, host = loaded("hook")
  local dir = env.lfs.dir
  env.lfs.dir = function(path)
    if path == E.req then
      host.clock = host.clock + (BUDGET_MS + 1) / 1000
    end
    return dir(path)
  end
  ping(E, "5-a")
  ping(E, "5-b")
  host.callbacks.onSimulationFrame()
  t.eq(taken(log), "5-a", "listing: a listing over the budget leaves room for the first request alone")
  local cost = costed(E, "5-a", "listing")
  t.eq(cost, "0.000", "listing: and the listing is the tick's cost, not the request's")
  env.lfs.dir = dir
  host.callbacks.onSimulationFrame()
  t.eq(entries(env, E.req), "", "listing: the second is answered on the next frame")
end

--------------------------------------------------------------------------------
-- The take is charged to the request
--------------------------------------------------------------------------------

do
  local E, env, _, host = loaded("hook")
  local open = env.io.open
  env.io.open = function(path, m)
    if m == "rb" and path:find("%.req$") then
      host.clock = host.clock + 0.002
    end
    return open(path, m)
  end
  spend(E, "6-a", 3)
  host.callbacks.onSimulationFrame()
  t.eq(costed(E, "6-a", "take"), "5.000", "take: the 2 ms the take spent and the 3 ms the op spent")
end

--------------------------------------------------------------------------------
-- The export host
--------------------------------------------------------------------------------

do
  local E, env, log, host = loaded("export")
  for i = 1, 4 do
    spend(E, "7-" .. i, 5)
  end
  rawget(env, "LuaExportAfterNextFrame")()
  t.eq(taken(log), "7-1 7-2", "export: two at 5 ms reach 10, and the tick takes no third")
  clear(log)
  rawget(env, "LuaExportAfterNextFrame")()
  t.eq(taken(log), "7-3 7-4", "export: the next frame takes the rest")
  local cost, v = costed(E, "7-4", "export")
  t.eq(v.tick, "2", "export: on the second frame")
  t.eq(cost, "5.000", "export: charged its own")
  t.eq(host.log, nil, "export: nothing was logged")
end

--------------------------------------------------------------------------------
-- No clock, no load
--------------------------------------------------------------------------------

local NO_CLOCK = {
  { "absent", nil },
  { "a number", 0 },
}

for _, state in ipairs({ "hook", "export" }) do
  for _, case in ipairs(NO_CLOCK) do
    local section = state .. ", os.clock " .. case[1]
    local host, box = sandboxed()
    local env = t.state(state, host)
    env.os.clock = case[2]
    t.load_executor(env)()
    t.eq(rawget(env, NAME), nil, section .. ": no namespace is published")
    t.eq(host.callbacks, nil, section .. ": nothing is registered")
    t.eq(env.lfs.attributes(box .. SAVED .. "Logs"), nil, section .. ": nothing is made")
    if state == "hook" then
      t.eq(host.log and #host.log, 1, section .. ": one dcs.log line")
      t.eq(host.log[1].message, "not loaded: os.clock is absent, so no tick can be held to its budget",
        section .. ": naming the clock")
    else
      t.eq(host.log, nil, section .. ": the export state has no log, so the refusal is silent")
    end
  end
end
