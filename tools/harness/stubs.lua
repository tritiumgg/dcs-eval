-- The DCS state models, checked against what they model.
--
-- Three things are proved here. The seven states carry exactly the
-- libraries the census read in each, and nothing else: one check per cell
-- of the table below, present or absent. `net.dostring_in` answers empty
-- and evaluates nothing, because a stub that evaluated would pass a chunk
-- the real state refuses. And every name `types/dcs.lua` declares is
-- modelled somewhere, so the definitions the language server checks the
-- executor against and the host the harness runs it in cannot drift apart
-- without this suite going red.
local t = ...

-- The census's table, as the frozen specification prints it: one row per
-- state, one column per library. This copy is the check and the module's
-- table is the build. They are kept apart on purpose: one table read by
-- both would agree with itself whatever it said.
local COLUMNS = { "require", "package", "io", "os", "lfs", "net", "debug", "loadstring" }
local SURFACE = {
  { "hook", { true, true, true, true, true, true, true, true } },
  { "gui", { true, true, true, true, true, true, true, true } },
  { "scripting", { true, true, true, true, true, false, true, true } },
  { "config", { true, true, true, true, true, true, true, true } },
  { "mission", { false, false, false, false, false, false, false, true } },
  { "missionscripting", { false, false, false, false, false, true, true, true } },
  { "export", { true, true, true, true, true, false, true, true } },
}

local FUNCTIONS = { require = true, loadstring = true }

for _, row in ipairs(SURFACE) do
  local state, cells = row[1], row[2]
  local env = t.state(state, {})
  for i, name in ipairs(COLUMNS) do
    if cells[i] then
      local want = FUNCTIONS[name] and "function" or "table"
      t.eq(type(env[name]), want, state .. "." .. name .. " is modelled")
    else
      t.raises(function()
        return env[name]
      end, "harness: " .. state .. "%." .. name .. " is not modelled$",
        state .. "." .. name .. " raises, naming itself")
    end
  end
end

-- The host tables: `DCS` in the hook state alone, `log` where the door
-- writes its markers, and neither in `export`.
local host = {}
local hook = t.state("hook", host)
local export = t.state("export", host)
local mission = t.state("mission", host)
t.eq(type(hook.DCS), "table", "the hook host has DCS")
t.eq(type(hook.log), "table", "the hook host has log")
t.eq(type(mission.log), "table", "mission has log")
t.raises(function()
  return export.DCS
end, "export%.DCS is not modelled", "export has no DCS")
t.raises(function()
  return export.net
end, "export%.net is not modelled", "export has no net")
t.raises(function()
  return hook.nosuch
end, "hook%.nosuch is not modelled", "an unknown global raises")
t.raises(function()
  return t.state("server", {})
end, "no DCS state named server", "server is not a state name here")

-- net.dostring_in answers empty and runs nothing. A stub that evaluated
-- would answer "2" to the first and raise on the second.
t.eq(hook.net.dostring_in("gui", "return 1 + 1"), "", "a chunk that would return a value answers empty")
t.eq(hook.net.dostring_in("mission", "error('would raise')"), "", "a chunk that would raise answers empty")
t.eq(hook.net.dostring_in("scripting", ""), "", "scripting is a state name")
t.eq(hook.net.dostring_in("config", ""), "", "config is a state name")
t.eq(hook.net.dostring_in("export", ""), "", "export is a state name")
t.eq(hook.net.dostring_in("nowhere", "return 1"), "Invalid state name", "a name DCS does not have")
t.eq(hook.net.dostring_in("missionscripting", ""), "Invalid state name",
  "missionscripting is reached under no name")
t.eq(t.state("missionscripting", {}).net.dostring_in("gui", "return 1"), "", "missionscripting carries the stub")
t.eq(t.state("gui", {}).net.dostring_in("gui", "return 1"), "", "gui carries the stub")
t.raises(function()
  return t.state("config", {}).net.dostring_in
end, "config%.net%.dostring_in is not modelled", "config's net has no members")
t.eq(hook.net.get_my_player_id(), 1, "the local player is 1")

-- os: stock time, ED's pid and temporary directory, and no way out.
t.eq(hook.os.getpid(), 4242, "the pid has a default")
host.pid = 7
t.eq(hook.os.getpid(), 7, "the pid is the host's")
t.eq(hook.os.tmpdir():sub(-1), "\\", "the temporary directory ends in a separator")
t.eq(type(hook.os.time()), "number", "os.time is stock")
t.raises(function()
  return hook.os.exit
end, "hook%.os%.exit is not modelled", "os.exit is not offered")
t.raises(function()
  return hook.os.execute
end, "hook%.os%.execute is not modelled", "os.execute is not offered")
t.raises(function()
  return hook.io.popen
end, "hook%.io%.popen is not modelled", "io.popen is not offered")
t.raises(function()
  hook.require("socket")
end, 'hook%.require%("socket"%) is not modelled', "require names what was asked for")

-- lfs over a sandbox. The executor writes with io.open and reads back with
-- lfs, so the two are checked against one tree.
local box = t.sandbox()
host.writedir = box .. "\\Saved Games\\DCS\\"
t.eq(hook.lfs.writedir(), host.writedir, "writedir is the host's")
t.eq(t.state("hook", {}).lfs.writedir(), [[C:\Users\harness\Saved Games\DCS\]], "writedir has a default")
t.eq(hook.lfs.tempdir():sub(-1), "\\", "tempdir ends in a separator")
t.eq(type(hook.lfs.currentdir()), "string", "currentdir answers")

t.eq(hook.lfs.attributes(box, "mode"), "directory", "the sandbox is a directory")
t.eq(hook.lfs.attributes(box .. "\\", "mode"), "directory", "a trailing separator is a directory still")
t.eq(hook.lfs.attributes(box .. "\\missing"), nil, "a missing path has no attributes")
t.eq(hook.lfs.mkdir(box .. "\\a\\b"), nil, "mkdir needs its parent")
t.eq(hook.lfs.attributes(box .. "\\a"), nil, "and made nothing on the way")
t.eq(hook.lfs.mkdir(box .. "\\a"), true, "mkdir makes a directory")
t.eq(hook.lfs.mkdir(box .. "\\a"), nil, "mkdir refuses one that exists")
t.eq(hook.lfs.mkdir(box .. "\\a\\b\\"), true, "mkdir takes a trailing separator")

local f = assert(hook.io.open(box .. "\\a\\req.txt", "wb"))
f:write("hello")
f:close()
t.eq(hook.lfs.attributes(box .. "\\a\\req.txt", "mode"), "file", "a written file is a file")
t.eq(hook.lfs.attributes(box .. "\\a\\req.txt").size, 5, "with its size")
local filled = {}
t.eq(hook.lfs.attributes(box .. "\\a\\req.txt", filled), filled, "a table is filled and returned")
t.eq(filled.mode, "file", "filled with the mode")
t.raises(function()
  return hook.lfs.attributes(box .. "\\a\\req.txt").modification
end, "%.modification is not modelled", "an attribute that is not modelled raises")

local names = {}
for name in hook.lfs.dir(box .. "\\a") do
  names[#names + 1] = name
end
t.eq(table.concat(names, " "), ". .. b req.txt", "dir lists the dots and the entries")
t.raises(function()
  hook.lfs.dir(box .. "\\nope")
end, "cannot open", "dir raises for a directory that is not there")
t.raises(function()
  hook.lfs.dir(box .. "\\a\\req.txt")
end, "cannot open", "dir raises for a file")
t.eq(hook.os.remove(box .. "\\a\\req.txt"), true, "os.remove is stock")
t.eq(hook.lfs.attributes(box .. "\\a\\req.txt"), nil, "and the file is gone")

-- DCS and log answer from the host and record into it.
local callbacks = {}
hook.DCS.setUserCallbacks(callbacks)
t.eq(host.callbacks, callbacks, "setUserCallbacks lands in the host")
t.eq(hook.DCS.getPause(), false, "not paused by default")
host.paused = true
t.eq(hook.DCS.getPause(), true, "paused when the host says")
hook.DCS.setPause(false)
t.eq(host.paused, false, "setPause writes the host")
t.eq(hook.DCS.getMissionName(), "", "no mission by default")
t.eq(hook.DCS.getMissionLoaded(), false, "nothing loaded by default")
t.raises(function()
  return hook.DCS.getSimulatorMode()
end, "set host%.simulator_mode", "the simulator mode has no default")
host.simulator_mode = "menu"
t.eq(hook.DCS.getSimulatorMode(), "menu", "the simulator mode is the host's")
t.raises(function()
  return hook.DCS.getPuase()
end, "hook%.DCS%.getPuase is not modelled", "a misspelt DCS read raises")
hook.log.write("EXECUTOR", hook.log.ERROR, "could not register")
mission.log.write("DOOR", mission.log.INFO, "marker")
t.eq(#host.log, 2, "log.write appends to the host")
t.eq(host.log[1].message, "could not register", "with the message")
t.eq(host.log[2].state, "mission", "and the state it was written from")
t.eq(hook.log.ERROR ~= hook.log.INFO, true, "the levels are distinct")

-- The executor may define things: a write is not a raise.
hook.DcsEvalExecutor = { version = 1 }
t.eq(hook.DcsEvalExecutor.version, 1, "a global the executor defines reads back")
t.eq(hook._G, hook, "_G is the model itself")

-- Every `table.member` the type definitions declare is modelled in at least
-- one state. Bare globals (the door's `a_do_script`) are modelled by the
-- task that builds the door, and are not looked for here.
local defs = assert(io.open(t.root .. "/types/dcs.lua", "rb"))
local source = defs:read("*a")
defs:close()
local models = {}
for _, row in ipairs(SURFACE) do
  models[#models + 1] = t.state(row[1], {})
end
local declared = 0
for tbl, member in source:gmatch("\nfunction ([%w_]+)%.([%w_]+)%(") do
  declared = declared + 1
  local found = false
  for _, env in ipairs(models) do
    local lib = rawget(env, tbl)
    if type(lib) == "table" and rawget(lib, member) ~= nil then
      found = true
    end
  end
  t.check(found, tbl .. "." .. member .. " is declared in types/dcs.lua and no state models it")
end
t.check(declared > 20, "the type definitions were read: " .. declared .. " members declared")
