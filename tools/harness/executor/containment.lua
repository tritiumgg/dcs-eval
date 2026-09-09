-- Containment and the choice of the two write roots, driven through the
-- host: `host.writedir`, `host.tempdir` and `host.cwd` are pointed where
-- each case needs and the namespace says what the executor chose.
--
-- Four things are proved. On the directories DCS hands back the output is
-- `<writedir>\Logs\DcsEval\<host>` and the transport root is
-- `<tempdir>\dcs-eval\<host>`, in both hosts, and nothing is created. A
-- temp candidate inside the install, relative, or inside `Saved Games` and
-- not under `Logs\` is refused and the transport goes to `<output>\rpc`,
-- while one under `Logs\` is kept. An output that fails the same test stops
-- the load: one `dcs.log` line, nothing registered, nothing published. And
-- an unreadable `lfs.writedir()` stops the load the same way, where an
-- unreadable `lfs.tempdir()` or `lfs.currentdir()` does not.
--
-- The mutations this suite exists to catch, and where each shows. Drop the
-- relative test and the relative section reads `lfs.tempdir` where it
-- wants the fallback. Drop the install test and the install section does
-- the same. Replace the `Logs` rule with a spelling test and the
-- `Logs\..\Config` case is admitted. Let `..` pop the drive and the
-- `C:\..\Program Files` case is admitted. Match a root without a segment
-- boundary and `LogsX` passes as `Logs`.
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

-- Load the executor into a fresh hook state over `host` and return the
-- namespace it published, or nil when it refused.
local function load(host, state)
  local env = t.state(state or "hook", host)
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
  t.eq(keys(host), 1, "hook: the callbacks are all the model saw, so nothing was created")
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
  local env = t.state("export", host)
  env.lfs.currentdir = function()
    error("no currentdir here", 0)
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(E and E.install_guard, "ABSENT", "export: an install guard that does not answer is reported absent")
  t.eq(E.chained, 4, "export: and the load went on to chain all four")
  t.eq(E.transport_source, "lfs.tempdir", "export: and the temp candidate is still kept")
end

