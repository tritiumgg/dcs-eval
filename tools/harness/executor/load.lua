-- The load shell, driven as both hosts from one file.
--
-- Four things are proved. Loaded into the hook state the file registers the
-- eighteen guarded callbacks and no `try` name, and publishes its namespace.
-- Loaded into the export state it chains the four `LuaExport*` globals onto
-- whatever held them, leaves a non-function holder alone, and never touches
-- `LuaExportActivityNextEvent`. Loaded anywhere else it registers nothing.
-- And the load runs under one `pcall`: with a `DCS.setUserCallbacks` that
-- raises, the file writes one `dcs.log` line, registers nothing, publishes
-- nothing, and leaves the harness's own state untouched.
--
-- Two mutations this suite exists to catch. Take the top-level `pcall` out
-- and the raise from `setUserCallbacks` escapes the load: the mutation
-- section goes red on its first check, with the raise's message. Detect the
-- host with `type(DCS)` instead of `rawget` and the export state raises
-- inside the load, where the `pcall` swallows it: nothing is chained, and
-- the export section goes red on its first read of `LuaExportStart`.
local t = ...

local NAME = "DcsEvalExecutor"

-- The check's copy of the list, kept apart from the executor's on purpose:
-- one table read by both would agree with itself whatever it said.
local HOOK_CALLBACKS = {
  "onMissionLoadBegin", "onMissionLoadProgress", "onMissionLoadEnd",
  "onSimulationStart", "onSimulationFrame", "onSimulationPause",
  "onSimulationResume", "onSimulationStop", "onPlayerChangeSlot",
  "onGameEvent", "onChatMessage", "onNetConnect", "onNetMissionChanged",
  "onShowGameMenu", "onShowBriefing", "onShowMissionEditor",
  "onShowMainInterface", "onShowMultiplayer",
}

local function keys(tbl)
  local n = 0
  for _ in pairs(tbl) do
    n = n + 1
  end
  return n
end

--------------------------------------------------------------------------------
-- The hook state
--------------------------------------------------------------------------------

do
  local host = {}
  local env = t.state("hook", host)
  t.load_executor(env)()

  t.eq(type(host.callbacks), "table", "hook: a callback table is registered")
  for _, name in ipairs(HOOK_CALLBACKS) do
    t.eq(type(host.callbacks[name]), "function", "hook: " .. name .. " is registered as a function")
  end
  t.eq(keys(host.callbacks), #HOOK_CALLBACKS, "hook: exactly the eighteen callbacks, no more")
  local tries = 0
  for name in pairs(host.callbacks) do
    if name:find("Try") then
      tries = tries + 1
    end
  end
  t.eq(tries, 0, "hook: no try variant is registered")
  t.eq(host.log, nil, "hook: a good load writes nothing to dcs.log")

  local E = rawget(env, NAME)
  t.eq(type(E), "table", "hook: the namespace global is published")
  t.eq(E.host, "hook", "hook: the namespace says which host")
  t.eq(E.phase, "menu", "hook: the phase starts at menu")

  local walk = {
    { "onMissionLoadBegin", "load" },
    { "onMissionLoadEnd", "load" },
    { "onSimulationStart", "sim" },
    { "onSimulationPause", "paused" },
    { "onSimulationResume", "sim" },
    { "onSimulationStop", "menu" },
  }
  for _, step in ipairs(walk) do
    host.callbacks[step[1]]()
    t.eq(E.phase, step[2], "hook: after " .. step[1] .. " the phase is " .. step[2])
  end

  for _, name in ipairs(HOOK_CALLBACKS) do
    host.callbacks[name]("x", 1, nil)
  end
  t.eq(E.raised, 0, "hook: every callback takes what DCS hands it without raising")

  local again = 0
  env.DCS.setUserCallbacks = function()
    again = again + 1
  end
  t.load_executor(env)()
  t.eq(again, 0, "hook: a second load into the same state registers nothing again")
  t.eq(rawget(env, NAME), E, "hook: the first load's namespace stands")
end

--------------------------------------------------------------------------------
-- The export state
--------------------------------------------------------------------------------

local EXPORT_CALLBACKS = {
  "LuaExportStart", "LuaExportBeforeNextFrame", "LuaExportAfterNextFrame", "LuaExportStop",
}

-- An empty Export.lua: nothing holds the four names before the load.
do
  local host = {}
  local env = t.state("export", host)
  t.load_executor(env)()

  for _, name in ipairs(EXPORT_CALLBACKS) do
    t.eq(type(rawget(env, name)), "function", "export: " .. name .. " is chained")
  end
  t.eq(rawget(env, "LuaExportActivityNextEvent"), nil, "export: LuaExportActivityNextEvent is never touched")

  local E = rawget(env, NAME)
  t.eq(type(E), "table", "export: the namespace global is published")
  t.eq(E.host, "export", "export: the namespace says which host")
  t.eq(E.phase, "loaded", "export: the phase starts at loaded")
  t.eq(E.chained, 4, "export: all four slots are counted as chained")

  rawget(env, "LuaExportStart")()
  t.eq(E.phase, "sim", "export: after LuaExportStart the phase is sim")
  rawget(env, "LuaExportBeforeNextFrame")()
  rawget(env, "LuaExportAfterNextFrame")()
  t.eq(E.phase, "sim", "export: the frame callbacks leave the phase alone")
  rawget(env, "LuaExportStop")()
  t.eq(E.phase, "stopped", "export: after LuaExportStop the phase is stopped")
  t.eq(E.raised, 0, "export: every callback takes a call without raising")
  t.eq(next(host), nil, "export: nothing in the model is written")
end

-- A crowded Export.lua: another exporter holds two of the names, one of them
-- raising, and something that is not a function holds a third.
do
  local host = {}
  local env = t.state("export", host)
  local seen = {}
  env.LuaExportStart = function(...)
    seen[#seen + 1] = select("#", ...)
  end
  env.LuaExportStop = function()
    error("theirs", 0)
  end
  env.LuaExportAfterNextFrame = "held"
  t.load_executor(env)()

  local E = rawget(env, NAME)
  t.eq(E.chained, 3, "export: a non-function holder is counted out")
  t.eq(rawget(env, "LuaExportAfterNextFrame"), "held", "export: the non-function holder is left alone")

  rawget(env, "LuaExportStart")(1, nil, 3)
  t.eq(#seen, 1, "export: the previous holder is still called")
  t.eq(seen[1], 3, "export: the previous holder sees every argument, nil holes kept")
  t.eq(E.phase, "sim", "export: ours ran alongside theirs")

  t.raises(function()
    rawget(env, "LuaExportStop")()
  end, "^theirs$", "export: a raise in the previous holder reaches DCS as it did before")
  t.eq(E.phase, "stopped", "export: ours ran before theirs raised")
end

--------------------------------------------------------------------------------
-- Neither host
--------------------------------------------------------------------------------

-- `config` is the state that catches a detection missing the `net` clause:
-- it has `net` and `lfs` and no `DCS`, which is the export signature but
-- for `net`. `scripting` is not here because its surface is the export
-- state's, and the file takes it for export; DCS never loads it there.
for _, state in ipairs({ "gui", "config", "missionscripting" }) do
  local host = {}
  local env = t.state(state, host)
  t.load_executor(env)()
  t.eq(rawget(env, NAME), nil, state .. ": no namespace is published")
  t.eq(rawget(env, "LuaExportStart"), nil, state .. ": nothing is chained")
  t.eq(next(host), nil, state .. ": nothing is registered and nothing is written")
end

-- `mission` has `log` and nothing else the file could use, so it is where
-- the "neither host" line is seen.
do
  local host = {}
  local env = t.state("mission", host)
  t.load_executor(env)()
  t.eq(rawget(env, NAME), nil, "mission: no namespace is published")
  t.eq(host.log and #host.log, 1, "mission: one dcs.log line")
  t.eq(host.log[1].subsystem, NAME, "mission: the line is under the file's name")
  t.eq(host.log[1].level, env.log.ERROR, "mission: the line is an error")
  t.check(host.log[1].message:find("no host", 1, true), "mission: the line says no host was found")
  t.eq(keys(host), 1, "mission: the line is all the model saw")
end

--------------------------------------------------------------------------------
-- The mutation: a registration that raises
--------------------------------------------------------------------------------

do
  local host = {}
  local env = t.state("hook", host)
  env.DCS.setUserCallbacks = function()
    error("boom\nline two")
  end
  local ok, err = pcall(t.load_executor(env))
  t.check(ok, "mutation: the load must not raise, but did: " .. tostring(err))
  t.eq(host.callbacks, nil, "mutation: nothing is registered")
  t.eq(rawget(env, NAME), nil, "mutation: no namespace is published")
  t.eq(host.log and #host.log, 1, "mutation: one dcs.log line")
  t.eq(host.log[1].subsystem, NAME, "mutation: the line is under the file's name")
  t.eq(host.log[1].level, env.log.ERROR, "mutation: the line is an error")
  t.check(host.log[1].message:find("boom", 1, true), "mutation: the line carries the reason")
  t.eq(host.log[1].message:find("\n", 1, true), nil, "mutation: the line is one line")
  t.eq(keys(host), 1, "mutation: the host table holds the log line and nothing else")
  t.eq(rawget(_G, NAME), nil, "mutation: the harness's own globals are untouched")
end
