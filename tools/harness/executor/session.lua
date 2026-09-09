-- The session: the stamp a load takes, driven through the host with
-- `host.pid` for the pid and the model's clock frozen where a path has to
-- be exact.
--
-- What is proved. The stamp is `<os.time()>-<os.getpid()>`, in both hosts,
-- and the namespace carries the time and the pid it was built from. Without
-- `os.getpid` the load stops: one `dcs.log` line where there is a log,
-- nothing registered, nothing published and nothing created; a pid that is
-- not a number stops it the same way.
--
-- The mutation this suite exists to catch. Drop the `os.getpid` test and
-- the load raises inside the stamp instead of refusing: the mutation section
-- still sees no namespace, but the line it reads no longer names
-- `os.getpid`, and a build that fenced with the clock alone would publish a
-- stamp of one number, which the shape check refuses.
local t = ...

local NAME = "DcsEvalExecutor"

-- The suite's own spellings of what a sandbox holds.
local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- A host whose directories are under a fresh sandbox, and the sandbox.
local function sandboxed(host)
  local box = t.sandbox()
  host = host or {}
  host.writedir = box .. SAVED
  host.tempdir = box .. TEMP
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
  local E = load(sandboxed({ pid = 7 }), "export", 1000)
  t.eq(E and E.stamp, "1000-7", "export: the stamp is the frozen clock and the host's pid")
  t.eq(E.started, 1000, "export: started is the clock")
  t.eq(E.pid, 7, "export: the pid is the host's")
  t.eq(E.chained, 4, "export: and the load went on to chain all four")
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
