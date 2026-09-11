-- The handshake: `<output>\executor.txt`, the file a client reads to find
-- the session, driven through the load over a sandbox the way
-- `executor/session` does, with the clock frozen so every value is exact.
--
-- What is proved. The load leaves one file under the output, named on the
-- namespace as `handshake`, and it is one envelope with no body: every
-- field of the specification's table, in its order, with the first line
-- naming this project. `protocol` is 2; `host`, `stamp` and `pid` are the
-- session's; `started` is the stamp's clock as a wall-clock time; the
-- four transport paths and the output are the ones the namespace holds;
-- `eval` is allowed and `ops` says `ping,eval`; `states` is one entry per
-- state the host answers, seven from the hook host and one from the
-- export host; `namespace` is the global and `source` the leaf of the file
-- loaded; `lfs_tempdir` is what DCS spelt, `transport_source` and
-- `install_guard` are what the roots chose, and the five figures and the
-- two limits are the constants. `app_version` is `_APP_VERSION` where it
-- is a string, `ABSENT` where there is none, and the type of anything
-- else, never called. Where the temp candidate falls back the file says
-- so and names the session under `rpc`; where a read did not answer the
-- field says `ABSENT`. The file goes down by rename, `.tmp` first, before
-- anything is registered, and a stale one from the last launch is
-- replaced whole with no `.tmp` left. A handshake that cannot be written
-- stops the load: one `dcs.log` line naming it, nothing registered,
-- nothing published; and so does a value the framer refuses, with the
-- header named, because the wire has no spelling for one.
--
-- The mutations this suite exists to catch. Drop `transport` or `stamp`
-- from the headers and the field-order check reads the list one short,
-- naming what is missing. Write `protocol: 1` and the protocol check
-- reads it. Write the file under its final name and the operation log
-- holds an open of it and no rename. Write it after registration and the
-- rename spy sees callbacks where it wants none. Append to the file
-- instead of replacing it and the stale case reads the old line first.
-- Call `_APP_VERSION` and the function case fails a check inside it.
-- Let a refused handshake through and the unwritable case reads a
-- namespace where it wants nil.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]
local OUTPUT = SAVED .. [[Logs\DcsEval\]]
local ROOT = TEMP .. [[dcs-eval\]]
local CWD = [[C:\Program Files\Eagle Dynamics\DCS World\bin]]
local CLOCK = 1000

-- The specification's table, in its order, with the identity line first.
-- The suite's own copy, kept apart from the executor's on purpose.
local FIELDS = {
  "executor", "protocol", "host", "stamp", "pid", "started", "transport",
  "req", "res", "arm", "output", "eval", "ops", "states", "namespace",
  "source", "lfs_tempdir", "transport_source", "install_guard",
  "tick_budget_ms", "instruction_budget", "instruction_ceiling",
  "probe_every", "quiet_s", "app_version", "max_request_bytes",
  "max_result_bytes",
}

local HOOK_STATES = "hook:carrier=local,returns=any,needs=always"
  .. " gui:carrier=dostring_in,returns=string,needs=menu"
  .. " scripting:carrier=dostring_in,returns=string,needs=menu"
  .. " mission:carrier=dostring_in,returns=string,needs=menu"
  .. " config:carrier=dostring_in,returns=string,needs=menu"
  .. " export:carrier=dostring_in,returns=string,needs=slot"
  .. " missionscripting:carrier=a_do_script,returns=string,needs=mission"
local EXPORT_STATES = "export:carrier=local,returns=any,needs=always"

-- A host whose directories are under a fresh sandbox, where the case did
-- not name them, and the sandbox.
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

local function mode(env, path)
  return env.lfs.attributes(path, "mode")
end

-- `rest` under `base`, a directory that exists, made through the model one
-- level at a time. Returns the path.
local function mkdirs(env, base, rest)
  local built = base
  for segment in rest:gmatch("[^\\/]+") do
    built = built .. "\\" .. segment
    if mode(env, built) ~= "directory" then
      assert(env.lfs.mkdir(built))
    end
  end
  return built
end

-- One file at `rest` under `base`, with its parents, written through the
-- model. Returns the path.
local function plant(env, base, rest, content)
  local dir, name = rest:match("^(.*)[\\/]([^\\/]+)$")
  local path = mkdirs(env, base, dir) .. "\\" .. name
  local fh = assert(env.io.open(path, "wb"))
  fh:write(content or "x")
  fh:close()
  return path
end

-- A state over a sandbox with the clock frozen and the pid the suite
-- chose, before the load, so a case can plant what an earlier launch left
-- or set what the state carries.
local function prepared(state, pid, host)
  local box
  host, box = sandboxed(host)
  host.pid = pid
  local env = t.state(state, host)
  env.os.time = function()
    return CLOCK
  end
  return env, box, host
end

-- A load with a frozen clock and a chosen pid, over a sandbox. Returns
-- the namespace it published, or nil, the state, the sandbox and the host.
local function session(state, pid, host)
  local env, box
  env, box, host = prepared(state, pid, host)
  t.load_executor(env)()
  return rawget(env, NAME), env, box, host
end

-- The handshake read back with the suite's own reader, not the executor's
-- parser: the names in the order written, the values by name, and the
-- body. Every header line must read `name: value`.
local function read(env, path)
  local fh = assert(env.io.open(path, "rb"))
  local bytes = fh:read("*a")
  fh:close()
  local blank = bytes:find("\n\n", 1, true)
  t.check(blank, "the file has a blank line ending its headers")
  local order, values = {}, {}
  for line in bytes:sub(1, blank):gmatch("([^\n]*)\n") do
    local name, value = line:match("^([A-Za-z0-9_%-]+): (.*)$")
    t.check(name, "every header line reads name: value, but one reads " .. line)
    order[#order + 1] = name
    values[name] = value
  end
  return order, values, bytes:sub(blank + 2), bytes
end

-- The names of `order` against FIELDS, one line, so a missing or extra
-- field reads as which.
local function fields(order)
  local want, got = table.concat(FIELDS, " "), table.concat(order, " ")
  t.eq(#order, #FIELDS, "every field of the table is present, and no other: " .. got)
  t.eq(got, want, "and in the table's order")
end

--------------------------------------------------------------------------------
-- The hook host
--------------------------------------------------------------------------------

do
  local E, env, box, host = session("hook", 7)
  local output = box .. OUTPUT .. "hook"
  t.eq(E and E.handshake, output .. [[\executor.txt]], "hook: the handshake is named under the output")
  t.eq(mode(env, E.handshake), "file", "hook: and is a file")
  t.eq(entries(env, output), "executor.txt", "hook: the output holds it and nothing else, no .tmp")
  local order, v, body = read(env, E.handshake)
  fields(order)
  t.eq(body, "", "hook: the envelope has no body")
  t.eq(v.executor, "dcs-eval", "hook: the first line names this project")
  t.eq(v.protocol, "2", "hook: protocol is 2")
  t.eq(v.host, "hook", "hook: the host")
  t.eq(v.stamp, "1000-7", "hook: the stamp is the session's")
  t.eq(v.pid, "7", "hook: the pid is the session's")
  t.eq(v.started, os.date("%Y-%m-%d %H:%M:%S", CLOCK), "hook: started is the stamp's clock as wall-clock time")
  t.eq(v.transport, box .. ROOT .. [[hook\1000-7]], "hook: transport is the session directory")
  t.eq(v.req, v.transport .. [[\req]], "hook: req is under it")
  t.eq(v.res, v.transport .. [[\res]], "hook: res is under it")
  t.eq(v.arm, v.transport .. [[\arm]], "hook: arm is under it")
  t.eq(v.output, output, "hook: output is the output directory")
  t.eq(v.eval, "allowed", "hook: eval is allowed")
  t.eq(v.ops, "ping,eval", "hook: the two ops")
  t.eq(v.states, HOOK_STATES, "hook: the seven states the hook host answers")
  t.eq(v.namespace, NAME, "hook: the namespace is the global")
  t.eq(v.source, "DcsEvalExecutor.lua", "hook: the source is the leaf of the file loaded")
  t.eq(v.lfs_tempdir, host.tempdir, "hook: lfs_tempdir is what DCS spelt, separator and all")
  t.eq(v.transport_source, "lfs.tempdir", "hook: the transport came from lfs.tempdir")
  t.eq(v.install_guard, CWD, "hook: the install guard is the working directory")
  t.eq(v.tick_budget_ms, "8", "hook: the tick budget")
  t.eq(v.instruction_budget, "1000000", "hook: the instruction budget")
  t.eq(v.instruction_ceiling, "50000000", "hook: the instruction ceiling")
  t.eq(v.probe_every, "8", "hook: the probe cadence")
  t.eq(v.quiet_s, "3", "hook: the quiet period")
  t.eq(v.app_version, "ABSENT", "hook: no _APP_VERSION is reported absent")
  t.eq(v.max_request_bytes, "262144", "hook: the request limit")
  t.eq(v.max_result_bytes, "65536", "hook: the reply ceiling")
  t.eq(host.log, nil, "hook: a good load writes nothing to dcs.log")
  t.eq(type(host.callbacks), "table", "hook: and the load went on to register")
end

--------------------------------------------------------------------------------
-- The export host
--------------------------------------------------------------------------------

do
  local E, env, box, host = session("export", 8)
  local output = box .. OUTPUT .. "export"
  t.eq(E and E.handshake, output .. [[\executor.txt]], "export: the handshake is under the export output")
  t.eq(entries(env, output), "executor.txt", "export: the output holds it and nothing else")
  local order, v, body = read(env, E.handshake)
  fields(order)
  t.eq(body, "", "export: the envelope has no body")
  t.eq(v.executor, "dcs-eval", "export: the first line names this project")
  t.eq(v.protocol, "2", "export: protocol is 2")
  t.eq(v.host, "export", "export: the host")
  t.eq(v.stamp, "1000-8", "export: the stamp is the session's")
  t.eq(v.transport, box .. ROOT .. [[export\1000-8]], "export: transport is the session under the export leaf")
  t.eq(v.output, output, "export: output is the export output")
  t.eq(v.states, EXPORT_STATES, "export: the one state the export host answers")
  t.eq(v.source, "DcsEvalExecutor.lua", "export: the source is the leaf of the file loaded")
  t.eq(v.install_guard, CWD, "export: the install guard where it answers")
  t.eq(v.lfs_tempdir, host.tempdir, "export: lfs_tempdir as DCS spelt it")
  t.eq(E.chained, 4, "export: and the load went on to chain all four")
end

--------------------------------------------------------------------------------
-- What the roots chose
--------------------------------------------------------------------------------

-- A temp candidate containment refuses: the session is under `rpc` beside
-- the output, and the raw candidate is still reported.
do
  local candidate = [[C:\Program Files\Eagle Dynamics\DCS World\bin\Temp\]]
  local E, env, box = session("hook", 7, { tempdir = candidate })
  local _, v = read(env, E.handshake)
  t.eq(v.transport_source, "fallback: beside the output", "fallback: the source says so")
  t.eq(v.transport, box .. OUTPUT .. [[hook\rpc\1000-7]], "fallback: transport is under rpc beside the output")
  t.eq(v.lfs_tempdir, candidate, "fallback: the raw candidate is still what DCS spelt")
  t.eq(v.output, box .. OUTPUT .. "hook", "fallback: the output is unmoved")
end

-- Reads that did not answer are reported absent.
do
  local E, env = session("hook", 7, { tempdir = "", cwd = "" })
  local _, v = read(env, E.handshake)
  t.eq(v.lfs_tempdir, "ABSENT", "absent: an unreadable lfs.tempdir is reported absent")
  t.eq(v.install_guard, "ABSENT", "absent: an unreadable lfs.currentdir is reported absent")
  t.eq(v.transport_source, "fallback: beside the output", "absent: and the transport fell back")
end

--------------------------------------------------------------------------------
-- The running build
--------------------------------------------------------------------------------

do
  local env = prepared("hook", 7)
  env._APP_VERSION = "2.9.28.26385"
  t.load_executor(env)()
  local E = rawget(env, NAME)
  local _, v = read(env, E.handshake)
  t.eq(v.app_version, "2.9.28.26385", "build: _APP_VERSION is reported as it is")
end

do
  local env = prepared("export", 7)
  env._APP_VERSION = function()
    t.check(false, "build: _APP_VERSION was called")
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  local _, v = read(env, E and E.handshake)
  t.eq(v.app_version, "function", "build: a _APP_VERSION that is not a string is reported by type, never called")
end

--------------------------------------------------------------------------------
-- How it reaches the disk
--------------------------------------------------------------------------------

-- The publish read off an operation log: the bytes go to `.tmp`, the
-- final name is removed, and the `.tmp` is renamed onto it; the final
-- name is never opened. At the rename nothing is registered yet, so a
-- client that reads the file finds a session made whole before DCS could
-- call in.
do
  local env, box, host = prepared("hook", 7)
  local final = box .. OUTPUT .. [[hook\executor.txt]]
  local log, at_rename = {}, {}
  local open, remove, rename = env.io.open, env.os.remove, env.os.rename
  env.io.open = function(path, m)
    log[#log + 1] = "open " .. m .. " " .. path
    return open(path, m)
  end
  env.os.remove = function(path)
    log[#log + 1] = "remove " .. path
    return remove(path)
  end
  env.os.rename = function(from, to)
    log[#log + 1] = "rename " .. from .. " " .. to
    at_rename.callbacks = host.callbacks
    at_rename.req = mode(env, box .. ROOT .. [[hook\1000-7\req]])
    at_rename.res = mode(env, box .. ROOT .. [[hook\1000-7\res]])
    return rename(from, to)
  end
  t.load_executor(env)()
  t.eq(table.concat(log, "\n"),
    "open wb " .. final .. ".tmp\nremove " .. final .. "\nrename " .. final .. ".tmp " .. final,
    "publish: the .tmp is opened, the final name removed, and the .tmp renamed onto it")
  t.eq(at_rename.callbacks, nil, "publish: at the rename nothing is registered yet")
  t.eq(at_rename.req, "directory", "publish: and req already exists")
  t.eq(at_rename.res, "directory", "publish: and so does res")
  t.eq(type(host.callbacks), "table", "publish: registration came after")
end

-- The last launch's handshake, and a `.tmp` it left, are replaced whole.
do
  local env, box = prepared("hook", 9)
  local stale = plant(env, box, OUTPUT .. [[hook\executor.txt]], "executor: dcs-eval\nstamp: 999-1\n\n")
  plant(env, box, OUTPUT .. [[hook\executor.txt.tmp]], "half")
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.handshake, stale, "stale: the new handshake is under the old name")
  local order, v, _, bytes = read(env, stale)
  fields(order)
  t.eq(v.stamp, "1000-9", "stale: the stamp is this launch's")
  t.eq(bytes:find("999-1", 1, true), nil, "stale: nothing of the old file remains")
  t.eq(entries(env, box .. OUTPUT .. "hook"), "executor.txt", "stale: the old .tmp is gone with it")
end

-- A second launch into the same install: the file says the second.
do
  local host, box = sandboxed({ pid = 7 })
  local env = t.state("hook", host)
  env.os.time = function()
    return CLOCK
  end
  t.load_executor(env)()
  host.pid = 8
  local again = t.state("hook", host)
  again.os.time = function()
    return CLOCK + 1
  end
  t.load_executor(again)()
  local _, v = read(again, box .. OUTPUT .. [[hook\executor.txt]])
  t.eq(v.stamp, "1001-8", "relaunch: the handshake names the second session")
  t.eq(v.transport, box .. ROOT .. [[hook\1001-8]], "relaunch: and its directory")
end

--------------------------------------------------------------------------------
-- A handshake that cannot be written
--------------------------------------------------------------------------------

-- A directory sits where the `.tmp` goes, so the open refuses: the load
-- stops with the handshake named, in both hosts.
for _, state in ipairs({ "hook", "export" }) do
  local env, box, host = prepared(state, 7)
  local blocked = mkdirs(env, box, OUTPUT .. state .. [[\executor.txt.tmp]])
  t.load_executor(env)()
  t.eq(rawget(env, NAME), nil, state .. " unwritable: no namespace is published")
  t.eq(host.callbacks, nil, state .. " unwritable: nothing is registered")
  t.eq(rawget(env, "LuaExportStart"), nil, state .. " unwritable: nothing is chained")
  t.eq(mode(env, blocked), "directory", state .. " unwritable: the directory in the way is left alone")
  t.eq(mode(env, box .. OUTPUT .. state .. [[\executor.txt]]), nil, state .. " unwritable: no handshake appeared")
  if state == "hook" then
    t.eq(host.log and #host.log, 1, "hook unwritable: one dcs.log line")
    t.eq(host.log[1].subsystem, NAME, "hook unwritable: under the file's name")
    t.eq(host.log[1].level, env.log.ERROR, "hook unwritable: an error")
    t.check(host.log[1].message:find("the handshake could not be published", 1, true),
      "hook unwritable: the line names the handshake: " .. host.log[1].message)
    t.check(host.log[1].message:find(blocked, 1, true), "hook unwritable: and the path that refused")
    t.eq(host.log[1].message:find("\n", 1, true), nil, "hook unwritable: the line is one line")
  else
    t.eq(host.log, nil, "export unwritable: nowhere to say so")
  end
end

-- A value the framer refuses: a byte past ASCII in what would be a header
-- stops the load with the header named, because the wire cannot carry it.
do
  local env, box, host = prepared("hook", 7)
  env._APP_VERSION = "2.9.28\255"
  t.load_executor(env)()
  t.eq(rawget(env, NAME), nil, "refused: no namespace is published")
  t.eq(host.callbacks, nil, "refused: nothing is registered")
  t.eq(mode(env, box .. OUTPUT .. [[hook\executor.txt]]), nil, "refused: no handshake appeared")
  t.eq(entries(env, box .. OUTPUT .. "hook"), "", "refused: and no .tmp either")
  t.eq(host.log and #host.log, 1, "refused: one dcs.log line")
  t.check(host.log[1].message:find("app_version: the value is not ASCII", 1, true),
    "refused: the line names the header and the rule: " .. host.log[1].message)
end
