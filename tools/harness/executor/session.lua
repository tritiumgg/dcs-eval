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
-- back beside the output, the way one that fails containment does.
--
-- The mutations this suite exists to catch. Drop the `os.getpid` test and
-- the load raises inside the stamp instead of refusing: the mutation section
-- still sees no namespace, but the line it reads no longer names
-- `os.getpid`, and a build that fenced with the clock alone would publish a
-- stamp of one number, which the shape check refuses. Make the arm file
-- along with the session and the arm check reads a file where it wants
-- nothing.
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
  t.eq(entries(env, box .. OUTPUT .. "hook"), "", "hook: and holds nothing yet")
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
