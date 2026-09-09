-- Containment and the choice of the two write roots, driven through the
-- host: `host.writedir`, `host.tempdir` and `host.cwd` are pointed where
-- each case needs and the namespace says what the executor chose.
--
-- Four things are proved. On the directories DCS hands back the output is
-- `<writedir>\Logs\DcsEval\<host>` and the transport root is
-- `<tempdir>\dcs-eval\<host>`, in both hosts. A temp candidate inside the
-- install, relative, or inside `Saved Games` and not under `Logs\` is
-- refused and the transport goes to `<output>\rpc`, while one under `Logs\`
-- is kept. An output that fails the same test stops the load: one `dcs.log`
-- line, nothing registered, nothing published. And an unreadable
-- `lfs.writedir()` stops the load the same way, where an unreadable
-- `lfs.tempdir()` or `lfs.currentdir()` does not.
--
-- The mutations this suite exists to catch, and where each shows. Drop the
-- relative test and the relative section reads `lfs.tempdir` where it
-- wants the fallback. Drop the install test and the install section does
-- the same. Replace the `Logs` rule with a spelling test and the
-- `Logs\..\Config` case is admitted. Let `..` pop the drive and the
-- `C:\..\Program Files` case is admitted. Match a root without a segment
-- boundary and `LogsX` passes as `Logs`. Append the boundary to a root
-- that already ends in one and a drive on its own contains nothing.
--
-- The suite proves the choice and runs over a stub filesystem that makes and
-- removes nothing. A load that gets past the choice creates its session
-- directory, and the cases here name real places, a drive root among them,
-- that no sandbox can stand in for. `executor/session` proves what is
-- created; here every path is a directory that exists and a call that would
-- change one raises.
local t = ...

local NAME = "DcsEvalExecutor"

-- The suite's own spellings, kept apart from the executor's and from the
-- harness defaults on purpose: a value read from either would agree with
-- itself whatever it said.
local WD = [[C:\Users\harness\Saved Games\DCS\]]
local TEMP = [[C:\Users\harness\AppData\Local\Temp\DCS\]]
local CWD = [[C:\Program Files\Eagle Dynamics\DCS World\bin]]
local OUTPUT = [[C:\Users\harness\Saved Games\DCS\Logs\DcsEval\hook]]
local FALLBACK = OUTPUT .. [[\rpc]]

local function keys(tbl)
  local n = 0
  for _ in pairs(tbl) do
    n = n + 1
  end
  return n
end

-- The stub filesystem: every path is a directory that exists, every
-- directory is empty, and the calls that would make or remove one raise.
local function refuse(call)
  return function()
    error("containment: " .. call .. " was called; this suite proves the choice and touches nothing", 0)
  end
end

local function dry(env)
  env.lfs.attributes = function(_, request)
    if request == "mode" then
      return "directory"
    end
    return { mode = "directory" }
  end
  env.lfs.dir = function()
    return function()
      return nil
    end
  end
  env.lfs.mkdir = refuse("lfs.mkdir")
  env.lfs.rmdir = refuse("lfs.rmdir")
  env.os.remove = refuse("os.remove")
  return env
end

-- Load the executor into a fresh hook state over `host` and return the
-- namespace it published, or nil when it refused.
local function load(host, state)
  local env = dry(t.state(state or "hook", host))
  t.load_executor(env)()
  return rawget(env, NAME), env
end

-- A temp candidate the executor must refuse: the transport falls back
-- beside the output, and the refusal is kept with the rule that made it.
local function falls_back(section, tempdir, rule)
  local host = { tempdir = tempdir }
  local E = load(host)
  t.eq(E and E.transport_source, "fallback: beside the output", section .. ": " .. tempdir .. " falls back")
  t.eq(E.transport_root, FALLBACK, section .. ": the fallback is rpc beside the output")
  t.eq(E.output, OUTPUT, section .. ": the output is unmoved")
  t.check(E.transport_refusal:find(rule, 1, true), section .. ": the refusal names the rule for " .. tempdir)
  t.eq(host.log, nil, section .. ": a fallback writes nothing to dcs.log")
end

-- A temp candidate the executor must keep.
local function keeps(section, tempdir, root)
  local E = load({ tempdir = tempdir })
  t.eq(E and E.transport_source, "lfs.tempdir", section .. ": " .. tempdir .. " is kept")
  t.eq(E.transport_root, root, section .. ": the transport root is under it")
  t.eq(E.transport_refusal, nil, section .. ": nothing was refused")
end

-- A write directory the executor must stop on: one error line under the
-- file's name naming the rule, and nothing else anywhere.
local function stops(section, host, rule)
  local before = keys(host)
  local E, env = load(host)
  t.eq(E, nil, section .. ": no namespace is published")
  t.eq(host.callbacks, nil, section .. ": nothing is registered")
  t.eq(host.log and #host.log, 1, section .. ": one dcs.log line")
  t.eq(host.log[1].subsystem, NAME, section .. ": the line is under the file's name")
  t.eq(host.log[1].level, env.log.ERROR, section .. ": the line is an error")
  t.check(host.log[1].message:find(rule, 1, true), section .. ": the line names the rule: " .. host.log[1].message)
  t.eq(host.log[1].message:find("\n", 1, true), nil, section .. ": the line is one line")
  t.eq(keys(host), before + 1, section .. ": the log line is all the model saw")
  t.eq(rawget(_G, NAME), nil, section .. ": the harness's own globals are untouched")
end

--------------------------------------------------------------------------------
-- The roots DCS hands back
--------------------------------------------------------------------------------

do
  local host = {}
  local E = load(host)
  t.eq(type(E), "table", "hook: the namespace is published")
  t.eq(E.output, OUTPUT, "hook: the output is under Logs in the write directory")
  t.eq(E.transport_root, TEMP .. [[dcs-eval\hook]], "hook: the transport root is under the temp directory")
  t.eq(E.transport_source, "lfs.tempdir", "hook: the transport came from lfs.tempdir")
  t.eq(E.transport_refusal, nil, "hook: nothing was refused")
  t.eq(E.install_guard, CWD, "hook: the install guard is the working directory")
  t.eq(E.lfs_tempdir, TEMP, "hook: the raw tempdir is kept as DCS spelt it")
  t.eq(host.log, nil, "hook: a good load writes nothing to dcs.log")
  t.eq(keys(host), 1, "hook: the callbacks are all the model saw")
end

do
  local host = {}
  local E = load(host, "export")
  t.eq(type(E), "table", "export: the namespace is published")
  t.eq(E.output, WD .. [[Logs\DcsEval\export]], "export: the output leaf is the host's")
  t.eq(E.transport_root, TEMP .. [[dcs-eval\export]], "export: the transport leaf is the host's")
  t.eq(E.transport_source, "lfs.tempdir", "export: the transport came from lfs.tempdir")
  t.eq(E.install_guard, CWD, "export: the install guard is the working directory where it answers")
  t.eq(next(host), nil, "export: nothing in the model is written")
end

-- The export state is not known to answer `lfs.currentdir()`. The guard is
-- reported absent and the load goes on without it.
do
  local host = {}
  local env = dry(t.state("export", host))
  env.lfs.currentdir = function()
    error("no currentdir here", 0)
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.install_guard, "ABSENT", "export: an install guard that does not answer is reported absent")
  t.eq(E.chained, 4, "export: and the load went on to chain all four")
  t.eq(E.transport_source, "lfs.tempdir", "export: and the temp candidate is still kept")
end

--------------------------------------------------------------------------------
-- Inside the install
--------------------------------------------------------------------------------

falls_back("install", CWD .. [[\Temp\]], "inside the install")
falls_back("install", [[c:\program files\EAGLE DYNAMICS\dcs world\BIN\Temp\]], "inside the install")
falls_back("install", [[C:\..\Program Files\Eagle Dynamics\DCS World\bin\Temp\]], "inside the install")
falls_back("install", [[C:/Program Files/Eagle Dynamics/DCS World/bin/Temp/]], "inside the install")
falls_back("install", CWD, "inside the install")
keeps("install", [[C:\Program Files\Eagle Dynamics\DCS World\bin2\]],
  [[C:\Program Files\Eagle Dynamics\DCS World\bin2\dcs-eval\hook]])

stops("install", { writedir = CWD .. [[\Saved Games\DCS\]] }, "inside the install")

-- With no install guard the install test cannot run, and the same candidate
-- is kept. The guard is the working directory and nothing else.
do
  local host = { tempdir = CWD .. [[\Temp\]] }
  local env = dry(t.state("hook", host))
  env.lfs.currentdir = function()
    return nil
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.install_guard, "ABSENT", "install: a nil working directory is an absent guard")
  t.eq(E.transport_source, "lfs.tempdir", "install: without the guard the candidate is kept")
end

--------------------------------------------------------------------------------
-- Relative
--------------------------------------------------------------------------------

falls_back("relative", [[Temp\]], "is relative")
falls_back("relative", [[.\Temp]], "is relative")
falls_back("relative", [[..\Temp]], "is relative")
falls_back("relative", [[C:Temp\]], "is relative")

stops("relative", { writedir = [[Saved Games\DCS\]] }, "is relative")

--------------------------------------------------------------------------------
-- Inside Saved Games and not under Logs
--------------------------------------------------------------------------------

local OUTSIDE = "inside Saved Games and not under Logs"

falls_back("saved games", WD .. [[Temp\]], OUTSIDE)
falls_back("saved games", WD .. [[Logs\..\Config\]], OUTSIDE)
falls_back("saved games", WD .. [[Logs\.\..\Config\]], OUTSIDE)
falls_back("saved games", WD .. [[Logs\Temp\..\..\Config\]], OUTSIDE)
falls_back("saved games", WD .. [[LogsX\]], OUTSIDE)
falls_back("saved games", WD .. [[Logs..\]], OUTSIDE)
falls_back("saved games", WD, OUTSIDE)
falls_back("saved games", WD:lower(), OUTSIDE)
falls_back("saved games", [[C:\Users\harness\Saved Games\DCS]], OUTSIDE)

keeps("saved games", WD .. [[Logs\Temp\]], WD .. [[Logs\Temp\dcs-eval\hook]])
keeps("saved games", WD .. [[Logs\]], WD .. [[Logs\dcs-eval\hook]])
keeps("saved games", WD:lower() .. [[logs\temp\]], WD:lower() .. [[logs\temp\dcs-eval\hook]])
keeps("saved games", WD .. [[Config\..\Logs\Temp\]], WD .. [[Config\..\Logs\Temp\dcs-eval\hook]])
keeps("saved games", [[C:\Users\harness\Saved Games\DCS.openbeta\Temp\]],
  [[C:\Users\harness\Saved Games\DCS.openbeta\Temp\dcs-eval\hook]])

-- A root that is a drive on its own has no segment for the boundary to sit
-- after, and `lfs` hands it back as `C:\`. It still holds the whole drive.
do
  local host = { cwd = [[D:\]], tempdir = [[D:\Temp\]] }
  local E = load(host)
  t.eq(E and E.transport_source, "fallback: beside the output", "drive root: an install at a drive root holds the candidate")
  t.check(E.transport_refusal:find("inside the install", 1, true), "drive root: and the refusal names the install")
end

-- With the install at the drive Saved Games is on, the output is inside it
-- too, and the load stops.
stops("drive root", { cwd = [[C:\]] }, "inside the install")

do
  local host = { writedir = [[C:\]] }
  local E = load(host)
  t.eq(E and E.output, [[C:\Logs\DcsEval\hook]], "drive root: a write directory at the drive root puts the output under its Logs")
  t.eq(E.transport_source, "fallback: beside the output", "drive root: and the temp directory is inside it and not under Logs")
  t.check(E.transport_refusal:find(OUTSIDE, 1, true), "drive root: with the Saved Games rule named")
end

-- The write directory's own spelling does not decide the test.
do
  local host = { writedir = WD:upper(), tempdir = WD .. [[Temp\]] }
  local E = load(host)
  t.eq(E and E.transport_source, "fallback: beside the output", "saved games: the write directory is matched case-folded")
  t.eq(E.output, WD:upper() .. [[Logs\DcsEval\hook]], "saved games: the output keeps the spelling DCS gave")
end

--------------------------------------------------------------------------------
-- Unreadable
--------------------------------------------------------------------------------

stops("unreadable", { writedir = false }, "lfs.writedir() is unreadable")
stops("unreadable", { writedir = "" }, "lfs.writedir() is unreadable")

do
  local host = {}
  local env = dry(t.state("hook", host))
  env.lfs.writedir = function()
    error("boom", 0)
  end
  t.load_executor(env)()
  t.eq(rawget(env, NAME), nil, "unreadable: a writedir that raises publishes nothing")
  t.eq(host.callbacks, nil, "unreadable: and registers nothing")
  t.check(host.log and host.log[1].message:find("lfs.writedir() is unreadable", 1, true),
    "unreadable: and the line says the write directory is unreadable")
end

-- The export state has no `log`, so the refusal there is silent: nothing
-- is chained, nothing is published, and the model saw nothing.
do
  local host = { writedir = false }
  local env = dry(t.state("export", host))
  t.load_executor(env)()
  t.eq(rawget(env, NAME), nil, "unreadable: the export state publishes nothing")
  t.eq(rawget(env, "LuaExportStart"), nil, "unreadable: and chains nothing")
  t.eq(host.log, nil, "unreadable: and has nowhere to say so")
  t.eq(keys(host), 1, "unreadable: the model holds what the suite set and nothing else")
end

do
  local host = { tempdir = false }
  local E = load(host)
  t.eq(E and E.transport_source, "fallback: beside the output", "unreadable: an unreadable tempdir falls back")
  t.eq(E.lfs_tempdir, "ABSENT", "unreadable: and is reported absent")
  t.check(E.transport_refusal:find("lfs.tempdir() is unreadable", 1, true), "unreadable: and the refusal says why")
  t.eq(host.log, nil, "unreadable: and nothing is written to dcs.log")
end

do
  local host = {}
  local env = dry(t.state("hook", host))
  env.lfs.tempdir = function()
    error("boom", 0)
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.transport_source, "fallback: beside the output", "unreadable: a tempdir that raises falls back")
  t.eq(type(host.callbacks), "table", "unreadable: and the load went on to register")
end
