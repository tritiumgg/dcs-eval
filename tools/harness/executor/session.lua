-- The session: the stamp a load takes and the directories it makes, driven
-- through the host with `host.writedir` and `host.tempdir` under a sandbox,
-- `host.pid` for the pid, and the model's clock frozen where a path has to
-- be exact.
--
-- What is proved. The stamp is `<os.time()>-<os.getpid()>`, in both hosts,
-- and the namespace carries the time and the pid it was built from. Without
-- `os.getpid` the load stops: one `dcs.log` line where there is a log,
-- nothing registered, nothing published and nothing created; a pid that is
-- not a number stops it the same way. The load makes the output directory
-- and `<root>\<stamp>\req` and `res`, with every missing parent, and names
-- the arm file without making it. An output that cannot be made stops the
-- load; a transport root from `lfs.tempdir()` that cannot be made falls
-- back beside the output, the way one that fails containment does. Before
-- the session is made every sibling directory under the root goes, with
-- the requests and replies in it, so a request in a foreign session is
-- never listed; the own stamp is kept, a file under the root is left, and
-- the other host's root is not touched. A sibling holding a file something
-- has open stays, named on the namespace and logged where there is a log,
-- and the load goes on; so does one deeper than a session goes.
--
-- The mutations this suite exists to catch. Drop the `os.getpid` test and
-- the load raises inside the stamp instead of refusing: the mutation section
-- still sees no namespace, but the line it reads no longer names
-- `os.getpid`, and a build that fenced with the clock alone would publish a
-- stamp of one number, which the shape check refuses. Make the arm file
-- along with the session and the arm check reads a file where it wants
-- nothing. Sweep with `os.remove` alone and the planted sibling stays.
-- Drop the own-stamp test and the own-stamp case reads its request gone.
-- Let a refusal stop the load and the held case has nothing registered.
local t = ...

local NAME = "DcsEvalExecutor"

-- The suite's own spellings of what a sandbox holds, and of the leaves the
-- executor puts under it.
local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]
local OUTPUT = SAVED .. [[Logs\DcsEval\]]
local ROOT = TEMP .. [[dcs-eval\]]
local CLOCK = 1000

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

-- Build one state over `host`, freeze its clock at `clock` when given, run
-- the executor, and return the namespace it published, or nil, and the
-- state.
local function load(host, state, clock)
  local env = t.state(state or "hook", host)
  if clock then
    env.os.time = function()
      return clock
    end
  end
  t.load_executor(env)()
  return rawget(env, NAME), env
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
-- level at a time the way the executor has to. Returns the path.
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

-- A load with a frozen clock and a pid the suite chose, over a sandbox.
local function session(state, pid, clock, host)
  local box
  host, box = sandboxed(host)
  host.pid = pid
  local E, env = load(host, state, clock)
  return E, env, box, host
end

--------------------------------------------------------------------------------
-- The stamp
--------------------------------------------------------------------------------

do
  local host = sandboxed()
  local before = os.time()
  local E = load(host)
  local after = os.time()
  t.eq(type(E), "table", "hook: the namespace is published")
  t.eq(E.pid, 4242, "hook: the pid is what os.getpid answered")
  t.check(E.started >= before and E.started <= after, "hook: started is the clock at load")
  t.eq(E.stamp, E.started .. "-4242", "hook: the stamp is the time, a dash, the pid")
  t.check(E.stamp:find("^%d+%-%d+$"), "hook: the stamp is two integers and nothing else")
  t.eq(host.log, nil, "hook: a good load writes nothing to dcs.log")
end

do
  local E = load(sandboxed({ pid = 7 }), "export", CLOCK)
  t.eq(E and E.stamp, "1000-7", "export: the stamp is the frozen clock and the host's pid")
  t.eq(E.started, CLOCK, "export: started is the clock")
  t.eq(E.pid, 7, "export: the pid is the host's")
  t.eq(E.chained, 4, "export: and the load went on to chain all four")
end

--------------------------------------------------------------------------------
-- The directories
--------------------------------------------------------------------------------

do
  local E, env, box = session("hook", 7, CLOCK)
  local root = box .. ROOT .. "hook"
  t.eq(E and E.session, root .. [[\1000-7]], "hook: the session is the stamp under the transport root")
  t.eq(E.req, E.session .. [[\req]], "hook: req is under the session")
  t.eq(E.res, E.session .. [[\res]], "hook: res is under the session")
  t.eq(E.arm, E.session .. [[\arm]], "hook: the arm file is named under the session")
  t.eq(mode(env, E.req), "directory", "hook: req is made")
  t.eq(mode(env, E.res), "directory", "hook: res is made")
  t.eq(mode(env, E.arm), nil, "hook: the arm file is not made")
  t.eq(entries(env, E.session), "req res", "hook: the session holds req and res and nothing else")
  t.eq(entries(env, root), "1000-7", "hook: the root holds the session and nothing else")
  t.eq(mode(env, box .. OUTPUT .. "hook"), "directory", "hook: the output directory is made")
  t.eq(entries(env, box .. OUTPUT .. "hook"), "executor.txt", "hook: and holds the handshake and nothing else")
  t.eq(E.transport_source, "lfs.tempdir", "hook: the transport root is still the temp candidate")
  t.eq(E.transport_refusal, nil, "hook: nothing was refused")
  t.eq(entries(env, box), "Saved Games Temp", "hook: the sandbox holds the two trees the host named")
end

do
  local E, env, box = session("export", 8, CLOCK)
  t.eq(E and E.session, box .. ROOT .. [[export\1000-8]], "export: the session is under the export leaf")
  t.eq(mode(env, E.req), "directory", "export: req is made")
  t.eq(mode(env, E.res), "directory", "export: res is made")
  t.eq(mode(env, E.arm), nil, "export: the arm file is not made")
  t.eq(mode(env, box .. OUTPUT .. "export"), "directory", "export: the output directory is under the export leaf")
  t.eq(mode(env, box .. ROOT .. "hook"), nil, "export: nothing is made under the hook leaf")
end

-- A temp candidate containment refuses: the session goes under `rpc`
-- beside the output, and nothing is made where the candidate pointed.
do
  local host = { tempdir = [[C:\Program Files\Eagle Dynamics\DCS World\bin\Temp\]] }
  local E, env, box = session("hook", 7, CLOCK, host)
  t.eq(E and E.transport_source, "fallback: beside the output", "fallback: the candidate fell back")
  t.eq(E.session, box .. OUTPUT .. [[hook\rpc\1000-7]], "fallback: the session is under rpc beside the output")
  t.eq(mode(env, E.req), "directory", "fallback: req is made there")
  t.eq(mode(env, E.res), "directory", "fallback: res is made there")
  t.eq(entries(env, box), "Saved Games", "fallback: nothing is made under the temp directory")
end

--------------------------------------------------------------------------------
-- A directory that cannot be made
--------------------------------------------------------------------------------

-- `Logs` is a file, so nothing under it can be made and the output stops
-- the load: one error line naming the path, nothing registered, nothing
-- published, and nothing made under the temp directory either, because the
-- output comes first.
do
  local host, box = sandboxed()
  local env = t.state("hook", host)
  plant(env, box, SAVED .. "Logs")
  t.load_executor(env)()
  t.eq(rawget(env, NAME), nil, "output: no namespace is published")
  t.eq(host.callbacks, nil, "output: nothing is registered")
  t.eq(host.log and #host.log, 1, "output: one dcs.log line")
  t.eq(host.log[1].level, env.log.ERROR, "output: the line is an error")
  t.check(host.log[1].message:find("could not be created", 1, true), "output: the line says what could not be made")
  t.check(host.log[1].message:find(box .. SAVED .. "Logs", 1, true), "output: and names the path that refused")
  t.eq(entries(env, box), "Saved Games", "output: nothing is made under the temp directory")
end

-- `Temp\DCS` is a file, so the temp candidate cannot be made: it falls
-- back beside the output, quietly, with the refusal kept.
do
  local host, box = sandboxed()
  local env = t.state("hook", host)
  plant(env, box, [[\Temp\DCS]])
  env.os.time = function()
    return CLOCK
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.transport_source, "fallback: beside the output", "temp: a candidate that cannot be made falls back")
  t.eq(E.transport_root, box .. OUTPUT .. [[hook\rpc]], "temp: to rpc beside the output")
  t.check(E.transport_refusal:find("could not be created", 1, true), "temp: the refusal says why")
  t.check(E.transport_refusal:find(box .. [[\Temp\DCS ]], 1, true), "temp: and names the path that refused, the file in the way")
  t.eq(E.session, E.transport_root .. [[\1000-4242]], "temp: the session is under the fallback")
  t.eq(mode(env, E.req), "directory", "temp: and req is made there")
  t.eq(mode(env, box .. [[\Temp\DCS]]), "file", "temp: the file in the way is left alone")
  t.eq(host.log, nil, "temp: a fallback writes nothing to dcs.log")
  t.eq(type(host.callbacks), "table", "temp: and the load went on to register")
end

-- Both the candidate and the fallback are files: there is nowhere left to
-- go, and the load stops naming the fallback.
do
  local host, box = sandboxed()
  local env = t.state("hook", host)
  plant(env, box, [[\Temp\DCS]])
  local rpc = plant(env, box, OUTPUT .. [[hook\rpc]])
  t.load_executor(env)()
  t.eq(rawget(env, NAME), nil, "both: no namespace is published")
  t.eq(host.callbacks, nil, "both: nothing is registered")
  t.eq(host.log and #host.log, 1, "both: one dcs.log line")
  t.eq(host.log[1].level, env.log.ERROR, "both: the line is an error")
  t.check(host.log[1].message:find("the transport root " .. rpc .. " could not be created", 1, true),
    "both: the line names the fallback, the last place tried: " .. host.log[1].message)
  t.eq(mode(env, rpc), "file", "both: the file in the way is left alone")
end

--------------------------------------------------------------------------------
-- The sweep
--------------------------------------------------------------------------------

-- A state over a sandbox with the clock frozen, before the load, so a case
-- can plant what an earlier session left.
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

-- Two sessions have ended, one holding a request and a reply. Both go, and
-- the request is not listed: its directory is not there to list.
do
  local env, box, host = prepared("hook", 7)
  local root = box .. ROOT .. "hook"
  plant(env, box, ROOT .. [[hook\1000-1\req\0001.req]], "for: 1000-1\n\nreturn 1")
  plant(env, box, ROOT .. [[hook\1000-1\res\0000.res]])
  mkdirs(env, box, ROOT .. [[hook\999-2]])
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.swept, 2, "sweep: both siblings are counted")
  t.eq(E.sweep_left, nil, "sweep: nothing was left")
  t.eq(mode(env, root .. [[\1000-1]]), nil, "sweep: the sibling with the request is gone")
  t.eq(mode(env, root .. [[\999-2]]), nil, "sweep: the empty sibling is gone")
  t.eq(entries(env, root), "1000-7", "sweep: the root holds this session and nothing else")
  t.eq(entries(env, E.req), "", "sweep: nothing is listed in req")
  t.eq(host.log, nil, "sweep: a clean sweep writes nothing to dcs.log")
end

-- A file directly under the root is not a session and is left alone.
do
  local env, box = prepared("hook", 7)
  local root = box .. ROOT .. "hook"
  plant(env, box, ROOT .. [[hook\stray.txt]])
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.swept, 0, "stray: a file is not a sibling")
  t.eq(entries(env, root), "1000-7 stray.txt", "stray: the file stays beside the session")
end

-- A directory already carrying this load's own stamp is kept with what is
-- in it, because the sweep goes by name and this is its own name.
do
  local env, box = prepared("hook", 7)
  local kept = plant(env, box, ROOT .. [[hook\1000-7\req\old.req]])
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.swept, 0, "own stamp: nothing is swept")
  t.eq(mode(env, kept), "file", "own stamp: the request under the own stamp is kept")
  t.eq(entries(env, E.session), "req res", "own stamp: res is made beside it")
end

-- The other host's root is a sibling of this one's, not under it.
do
  local env, box = prepared("hook", 7)
  local theirs = plant(env, box, ROOT .. [[export\1000-1\req\0001.req]])
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.swept, 0, "other host: nothing under the other root is a sibling")
  t.eq(mode(env, theirs), "file", "other host: the export session is untouched")
end

-- A second launch: the first session is swept and the second kept.
do
  local host, box = sandboxed({ pid = 7 })
  local root = box .. ROOT .. "hook"
  local first = load(host, "hook", CLOCK)
  host.pid = 8
  local second, env = load(host, "hook", CLOCK + 1)
  t.eq(first and first.stamp, "1000-7", "relaunch: the first session")
  t.eq(second and second.stamp, "1001-8", "relaunch: the second session")
  t.eq(second.swept, 1, "relaunch: the first is swept")
  t.eq(mode(env, first.session), nil, "relaunch: and is gone")
  t.eq(entries(env, root), "1001-8", "relaunch: the root holds the second and nothing else")
end

-- A sibling a client still holds: the suite keeps one of its files open
-- through the load. It stays, is named with what refused, and is logged in
-- the hook state, and the load goes on either way.
for _, state in ipairs({ "hook", "export" }) do
  local env, box, host = prepared(state, 7)
  local root = box .. ROOT .. state
  local held_path = plant(env, box, ROOT .. state .. [[\2000-9\req\held.req]])
  mkdirs(env, box, ROOT .. state .. [[\2000-8\res]])
  local held = assert(io.open(held_path, "rb"))
  local ok, err = pcall(t.load_executor(env))
  held:close()
  t.check(ok, state .. " held: the load must not raise, but did: " .. tostring(err))
  local E = rawget(env, NAME)
  t.eq(E and E.swept, 1, state .. " held: the sibling nothing holds is swept")
  t.eq(E.sweep_left and #E.sweep_left, 1, state .. " held: one sibling is left")
  t.check(E.sweep_left[1]:find("^2000%-9: "), state .. " held: it is named: " .. E.sweep_left[1])
  t.check(E.sweep_left[1]:find(held_path, 1, true), state .. " held: with the path that refused")
  t.eq(mode(env, held_path), "file", state .. " held: the held file stays")
  t.eq(entries(env, root), "1000-7 2000-9", state .. " held: the root holds this session and the one left")
  if state == "hook" then
    t.eq(host.log and #host.log, 1, "hook held: one dcs.log line")
    t.eq(host.log[1].subsystem, NAME, "hook held: under the file's name")
    t.eq(host.log[1].level, env.log.WARNING, "hook held: a warning, not an error")
    t.check(host.log[1].message:find("2000-9", 1, true), "hook held: naming the sibling")
    t.eq(type(host.callbacks), "table", "hook held: and the load went on to register")
  else
    t.eq(host.log, nil, "export held: nowhere to log, so the namespace is where it shows")
    t.eq(E.chained, 4, "export held: and the load went on to chain")
  end
end

-- A sibling deeper than a session goes is not something a session made.
-- It is left, named, and the load goes on.
do
  local env, box, host = prepared("hook", 7)
  local deep = plant(env, box, ROOT .. [[hook\3000-1\req\deeper\x.txt]])
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.swept, 0, "deep: the sibling is not swept")
  t.check(E.sweep_left and E.sweep_left[1]:find("deeper than a session", 1, true),
    "deep: and the reason says so: " .. tostring(E.sweep_left and E.sweep_left[1]))
  t.eq(mode(env, deep), "file", "deep: what lies below is untouched")
  t.eq(host.log and #host.log, 1, "deep: one dcs.log line")
end

--------------------------------------------------------------------------------
-- The mutation: no os.getpid
--------------------------------------------------------------------------------

-- Two ways the pid can be missing: no `os.getpid` at all, and one that
-- answers something that is not a number. Each stops the load in both hosts.
local ABSENT = {
  { "absent", nil },
  {
    "a string",
    function()
      return "7"
    end,
  },
}

for _, state in ipairs({ "hook", "export" }) do
  for _, case in ipairs(ABSENT) do
    local section = state .. ", os.getpid " .. case[1]
    local host, box = sandboxed()
    local env = t.state(state, host)
    env.os.getpid = case[2]
    t.load_executor(env)()
    t.eq(rawget(env, NAME), nil, section .. ": no namespace is published")
    t.eq(host.callbacks, nil, section .. ": nothing is registered")
    t.eq(rawget(env, "LuaExportStart"), nil, section .. ": nothing is chained")
    t.eq(entries(env, box), "", section .. ": nothing is created")
    if state == "hook" then
      t.eq(host.log and #host.log, 1, section .. ": one dcs.log line")
      t.eq(host.log[1].subsystem, NAME, section .. ": the line is under the file's name")
      t.eq(host.log[1].level, env.log.ERROR, section .. ": the line is an error")
      t.check(host.log[1].message:find("os.getpid", 1, true), section .. ": the line names os.getpid")
      t.eq(host.log[1].message:find("\n", 1, true), nil, section .. ": the line is one line")
    else
      t.eq(host.log, nil, section .. ": the export state has nowhere to say so")
    end
    t.eq(rawget(_G, NAME), nil, section .. ": the harness's own globals are untouched")
  end
end
