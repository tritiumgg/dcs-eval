-- The executor: the one Lua file DCS loads, and the whole of the in-game
-- half. ADR 0002 gives it its name.
--
-- DCS runs this file in two Lua states. At launch it runs every file in
-- `Saved Games\DCS\Scripts\Hooks\` in the hook state, where `DCS` is a table
-- and `DCS.setUserCallbacks` is the way in. At each mission start it runs
-- `Saved Games\DCS\Scripts\Export.lua` in the export state, where one
-- `dofile` line points here; that state has no `DCS` and no `net`, and the
-- way in is the four `LuaExport*` globals, which other exporters also hold.
-- The top level tells the two apart and registers the tail the state wants.
-- Everything past registration is one implementation.
--
-- The whole top level runs under one `pcall`. An installed executor must
-- never be the reason DCS fails to start, so a build on which the load
-- raises writes one line to `dcs.log` and registers nothing.
--
-- Every read of a host global goes through `rawget(_G, name)`. Off DCS this
-- file runs under a harness whose state models raise on a name they do not
-- carry, so `type(DCS)` in the export state would raise where DCS itself
-- answers nil. `rawget` reads the same answer in both.

local NAME = "DcsEvalExecutor"

-- The hook callbacks registered, and the phase each one moves the executor
-- to, where it moves one. Every callback is guarded, so a raise inside it
-- never reaches DCS, and none returns a value. The `try*` variants are not
-- here on purpose: their return value overrides DCS's own handling and every
-- other hook checking the same thing, so registering one would change
-- behaviour for every tool on the machine. The names past the simulation set
-- were offered in a measured session and not all seen to fire, which is "did
-- not fire" rather than "does not exist"; each costs one table entry.
local HOOK_CALLBACKS = {
  { "onMissionLoadBegin", "load" },
  { "onMissionLoadProgress" },
  { "onMissionLoadEnd" },
  { "onSimulationStart", "sim" },
  { "onSimulationFrame" },
  { "onSimulationPause", "paused" },
  { "onSimulationResume", "sim" },
  { "onSimulationStop", "menu" },
  { "onPlayerChangeSlot" },
  { "onGameEvent" },
  { "onChatMessage" },
  { "onNetConnect" },
  { "onNetMissionChanged" },
  { "onShowGameMenu" },
  { "onShowBriefing" },
  { "onShowMissionEditor" },
  { "onShowMainInterface" },
  { "onShowMultiplayer" },
}

-- The export globals chained onto. `LuaExportActivityNextEvent` is not here:
-- it returns the time DCS should call it next, and DCS acts on that return.
local EXPORT_CALLBACKS = {
  { "LuaExportStart", "sim" },
  { "LuaExportBeforeNextFrame" },
  { "LuaExportAfterNextFrame" },
  { "LuaExportStop", "stopped" },
}

-- The namespace: what this file publishes as the global `DcsEvalExecutor`,
-- and only once registration has succeeded, so a failed load leaves no
-- trace of itself. `last_raise` appears on the first raise a guard catches.
local E

local function nothing() end

-- One wrapper per callback, built here once. DCS calls the wrapper and the
-- wrapper pcalls a body that already exists. Nothing is allocated per call,
-- because one of these is `onSimulationFrame` and the dormant budget is
-- counted in VM instructions. A raise in the body is counted and kept, not
-- rethrown: a raise escaping into a DCS callback takes the session with it.
local function guard(name, phase)
  local body = nothing
  if phase then
    body = function()
      E.phase = phase
    end
  end
  return function(...)
    local ok, err = pcall(body, ...)
    if not ok then
      E.raised = E.raised + 1
      E.last_raise = name .. ": " .. tostring(err)
    end
  end
end

local function register_hook(DCS)
  local callbacks = {}
  for _, row in ipairs(HOOK_CALLBACKS) do
    callbacks[row[1]] = guard(row[1], row[2])
  end
  DCS.setUserCallbacks(callbacks)
end

-- Chain onto one export global. `Export.lua` is a shared file that SRS,
-- Tacview and force-feedback drivers also edit, so the previous holder is
-- kept and called after ours, unprotected and as a tail call: a raise in
-- somebody else's exporter reaches DCS exactly as it did before this file
-- was installed, under their traceback rather than this file's name. Ours
-- runs first, so it has run whether or not theirs raises. A holder that is
-- not a function is left alone, because calling it would raise every frame
-- and replacing it would discard whatever was meant by it. The number of
-- slots chained is published, so three of four is visible rather than
-- claimed as four.
local function chain(name, phase)
  local previous = rawget(_G, name)
  if previous ~= nil and type(previous) ~= "function" then
    return false
  end
  local ours = guard(name, phase)
  if previous then
    rawset(_G, name, function(...)
      ours(...)
      return previous(...)
    end)
  else
    rawset(_G, name, ours)
  end
  return true
end

local function register_export()
  local chained = 0
  for _, row in ipairs(EXPORT_CALLBACKS) do
    if chain(row[1], row[2]) then
      chained = chained + 1
    end
  end
  E.chained = chained
end

-- Which state this file woke in, from facts that hold in each: `DCS` is a
-- table in the hook state and nil in export, `net` is nil in export, and
-- `lfs` is present in both. Any other state gets neither, the load stops
-- here, and the raise says what it saw.
local function detect()
  local DCS = rawget(_G, "DCS")
  local net = rawget(_G, "net")
  local lfs = rawget(_G, "lfs")
  if type(DCS) == "table" and type(DCS.setUserCallbacks) == "function" then
    return "hook", DCS
  elseif DCS == nil and net == nil and type(lfs) == "table" then
    return "export"
  end
  error("no host: DCS is " .. type(DCS) .. ", net is " .. type(net) .. ", lfs is " .. type(lfs), 0)
end

local function main()
  -- Loaded once per state. DCS runs `Export.lua` at every mission start and
  -- whether the export state survives between missions is not measured; a
  -- second run here would chain onto this file's own wrapper. A second copy
  -- of the file under another name in `Scripts\Hooks\` would register the
  -- callbacks twice. Either way the first load stands.
  if rawget(_G, NAME) then
    return
  end
  local host, DCS = detect()
  E = { host = host, phase = host == "hook" and "menu" or "loaded", raised = 0 }
  if host == "hook" then
    register_hook(DCS)
  else
    register_export()
  end
  rawset(_G, NAME, E)
end

-- The one `pcall`. A load that raises anywhere above writes one line to
-- `dcs.log`, under this file's name at level ERROR with the reason on one
-- line, and nothing else happens: no callbacks, no global. Where the state
-- has no `log` (the export state has none) there is nowhere to write before
-- the transport exists, and the failure is silent until something outside
-- asks. The handler is itself guarded, so it cannot be the raise that
-- escapes.
local ok, err = pcall(main)
if not ok then
  pcall(function()
    local log = rawget(_G, "log")
    if type(log) == "table" and type(log.write) == "function" then
      log.write(NAME, log.ERROR, "not loaded: " .. (tostring(err):gsub("[\r\n]+", " ")))
    end
  end)
end
