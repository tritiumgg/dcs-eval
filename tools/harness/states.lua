-- The seven DCS Lua states, modelled: what a suite hands the executor as its
-- globals in place of the state DCS would have loaded it into.
--
-- Each model is a strict table (the runner's `t.strict`). It carries the stock
-- Lua 5.1 base and the libraries the census read in that state, and any other
-- name raises where it is read, naming the state and the name. The states
-- differ only in their libraries and SURFACE below is the whole of that
-- difference: `mission` and `missionscripting` are sanitised, with no
-- `require`, `package`, `io`, `os` or `lfs`; `config` holds a `net` table
-- with no members; `export` has neither `net` nor `DCS`.
--
-- `net.dostring_in` never evaluates. It answers `''` for a state DCS would
-- accept and `'Invalid state name'` for any other, because the reference
-- interpreter has one state, and a stub that ran the chunk here would turn a
-- probe that fails in DCS into one that passes under the harness.
--
-- `lfs` is not in stock Lua, so it is modelled over the interpreter's own
-- `io` and `os` against the real filesystem: the executor writes files with
-- `io.open` and lists them with `lfs.dir`, and the two must agree on one
-- directory tree. `io` and `os` are the stock libraries with the members
-- that reach past a sandbox left out (`os.exit` would end the runner,
-- `os.execute` and `io.popen` would shell out, `io.write` would print into
-- the runner's own output) plus ED's `os.getpid` and `os.tmpdir`. `debug` is
-- the stock library whole.
--
-- `host` is the suite's own table and the model reads it live. The directory
-- and process reads answer from it (`writedir`, `tempdir`, `tmpdir`, `cwd`,
-- `pid`, `paused`, `mission_name`, ...), and what the executor hands to
-- `DCS.setUserCallbacks` and `log.write` lands in `host.callbacks` and
-- `host.log`, where the suite can look at it.

-- Where the directory and process reads point when the suite does not say:
-- the shape of a real install, under a user that does not exist, so a model
-- written to by mistake fails at the filesystem rather than in a real
-- `Saved Games`.
local DEFAULT = {
  writedir = [[C:\Users\harness\Saved Games\DCS\]],
  tempdir = [[C:\Users\harness\AppData\Local\Temp\DCS\]],
  tmpdir = [[C:\Users\harness\AppData\Local\Temp\]],
  cwd = [[C:\Program Files\Eagle Dynamics\DCS World\bin]],
  pid = 4242,
}

local function answer(host, key)
  local v = host[key]
  if v == nil then
    return DEFAULT[key]
  end
  return v
end

-- The stock base every state shares. `dofile` and `loadfile` are left out
-- on purpose: the executor is one file that loads nothing else, and a read
-- of either is a raise that says so.
local BASE = {
  "assert", "collectgarbage", "error", "getfenv", "getmetatable", "ipairs",
  "load", "next", "pairs", "pcall", "rawequal", "rawget", "rawset", "select",
  "setfenv", "setmetatable", "tonumber", "tostring", "type", "unpack",
  "xpcall", "_VERSION", "string", "table", "math", "coroutine",
}

-- One row per state: the library names the census reached there, plus the
-- host tables (`DCS`, `log`) where they are known to exist.
local SURFACE = {
  hook = { "require", "package", "io", "os", "lfs", "net", "debug", "loadstring", "DCS", "log" },
  gui = { "require", "package", "io", "os", "lfs", "net", "debug", "loadstring" },
  scripting = { "require", "package", "io", "os", "lfs", "debug", "loadstring" },
  config = { "require", "package", "io", "os", "lfs", "net", "debug", "loadstring" },
  mission = { "loadstring", "log" },
  missionscripting = { "net", "debug", "loadstring" },
  export = { "require", "package", "io", "os", "lfs", "debug", "loadstring" },
}

local STATES = { "hook", "gui", "scripting", "config", "mission", "missionscripting", "export" }

local function pick(lib, names)
  local m = {}
  for _, k in ipairs(names) do
    m[k] = lib[k]
  end
  return m
end

local function copy(lib)
  local m = {}
  for k, v in pairs(lib) do
    m[k] = v
  end
  return m
end

--------------------------------------------------------------------------------
-- net
--------------------------------------------------------------------------------

-- The names DCS accepts on the far side of `net.dostring_in`. `server` is a
-- second name for the `scripting` state, and DCS answers to both; the model
-- of the state is one table, under the first.
local DOSTRING_STATES = {
  gui = true,
  scripting = true,
  server = true,
  mission = true,
  config = true,
  export = true,
}

local function dostring_in(state, _)
  if DOSTRING_STATES[state] then
    return ""
  end
  return "Invalid state name"
end

--------------------------------------------------------------------------------
-- lfs over the real filesystem
--------------------------------------------------------------------------------

-- Without its trailing separators: `io.open` reports a directory named with
-- one as missing rather than as a directory.
local function strip(path)
  local p = path:gsub("[\\/]+$", "")
  return p
end

-- Renaming a path to itself succeeds for a file and for a directory, and
-- fails for nothing else, so it is the existence test the interpreter has.
-- It also fails for a file something holds open, which Windows will not
-- rename either; such a file still opens for reading, so that is the second
-- test.
local function present(path)
  if os.rename(path, path) then
    return true
  end
  local fh = io.open(path, "rb")
  if fh then
    fh:close()
    return true
  end
  return false
end

-- A file opens for reading; a directory that exists refuses to. Only `mode`
-- and `size` are modelled, and a read of any other attribute raises, naming
-- it, so a use of one is a decision rather than a nil.
local function attributes(t, state, path, request)
  local p = strip(path)
  if not present(p) then
    return nil, path .. ": No such file or directory"
  end
  local attr
  local fh = io.open(p, "rb")
  if fh then
    attr = { mode = "file", size = fh:seek("end") }
    fh:close()
  else
    attr = { mode = "directory", size = 0 }
  end
  attr = t.strict(state .. ".lfs.attributes(" .. path .. ")", attr)
  if type(request) == "string" then
    return attr[request]
  end
  if type(request) == "table" then
    for k, v in pairs(attr) do
      request[k] = v
    end
    return request
  end
  return attr
end

-- `dir /b` prints nothing for a directory that is not there, so existence is
-- checked first and the raise is the library's own.
local function dir(t, state, path)
  local p = strip(path)
  if attributes(t, state, p, "mode") ~= "directory" then
    error("cannot open " .. path, 2)
  end
  local names = { ".", ".." }
  local pipe = assert(io.popen('dir /b /a "' .. p .. '" 2>nul'))
  for line in pipe:lines() do
    names[#names + 1] = line
  end
  pipe:close()
  local i = 0
  return function()
    i = i + 1
    return names[i]
  end
end

-- cmd's `mkdir` creates every missing parent and the library's does not, so
-- the parent is checked first.
local function mkdir(t, state, path)
  local p = strip(path)
  local parent = p:match("^(.+)[\\/][^\\/]+$")
  if parent and attributes(t, state, parent, "mode") ~= "directory" then
    return nil, "No such file or directory"
  end
  if present(p) then
    return nil, "File exists"
  end
  if os.execute('mkdir "' .. p .. '" >nul 2>&1') ~= 0 then
    return nil, "cannot make directory"
  end
  return true
end

-- One empty directory, like the library's: a directory with entries is
-- refused before the shell is asked. cmd's `rmdir` does not reliably say
-- whether it removed anything, so the answer is whether the path is still
-- there afterwards, which is what a handle held on it looks like.
local function rmdir(t, state, path)
  local p = strip(path)
  if attributes(t, state, p, "mode") ~= "directory" then
    return nil, "No such file or directory"
  end
  for name in dir(t, state, p) do
    if name ~= "." and name ~= ".." then
      return nil, "Directory not empty"
    end
  end
  os.execute('rmdir "' .. p .. '" >nul 2>&1')
  if present(p) then
    return nil, "Permission denied"
  end
  return true
end

--------------------------------------------------------------------------------
-- The libraries, one builder each
--------------------------------------------------------------------------------

local MAKE = {}

function MAKE.require(state)
  return function(name)
    error("harness: " .. state .. ".require(" .. string.format("%q", tostring(name)) .. ") is not modelled", 2)
  end
end

function MAKE.package(state, _, t)
  return t.strict(state .. ".package", {})
end

function MAKE.loadstring()
  return loadstring
end

function MAKE.debug(state, _, t)
  return t.strict(state .. ".debug", copy(debug))
end

function MAKE.io(state, _, t)
  return t.strict(state .. ".io", pick(io, { "open", "lines", "close", "type" }))
end

function MAKE.os(state, host, t)
  local m = pick(os, { "clock", "date", "difftime", "remove", "rename", "time" })
  m.getpid = function()
    return answer(host, "pid")
  end
  m.tmpdir = function()
    return answer(host, "tmpdir")
  end
  return t.strict(state .. ".os", m)
end

function MAKE.lfs(state, host, t)
  return t.strict(state .. ".lfs", {
    attributes = function(path, request)
      return attributes(t, state, path, request)
    end,
    dir = function(path)
      return dir(t, state, path)
    end,
    mkdir = function(path)
      return mkdir(t, state, path)
    end,
    rmdir = function(path)
      return rmdir(t, state, path)
    end,
    currentdir = function()
      return answer(host, "cwd")
    end,
    writedir = function()
      return answer(host, "writedir")
    end,
    tempdir = function()
      return answer(host, "tempdir")
    end,
  })
end

function MAKE.net(state, _, t)
  if state == "config" then
    return t.strict(state .. ".net", {})
  end
  return t.strict(state .. ".net", {
    dostring_in = dostring_in,
    get_my_player_id = function()
      return 1
    end,
  })
end

-- The hook host's table. Its reads answer from `host`, so a suite can pause
-- the simulator or load a mission by setting a field and watch the executor
-- react; unset, the answers describe a simulator at the main menu with
-- nothing loaded. The simulator mode has no default: its vocabulary is
-- unmeasured, and a suite that needs one says which.
local DCS_READS = {
  getPause = { "paused", false },
  getRealTime = { "real_time", 0 },
  getModelTime = { "model_time", 0 },
  getMissionName = { "mission_name", "" },
  getMissionFilename = { "mission_filename", "" },
  getMissionDescription = { "mission_description", "" },
  getMissionLoaded = { "mission_loaded", false },
  isMultiplayer = { "multiplayer", false },
  isServer = { "server", false },
  isTrackPlaying = { "track_playing", false },
}

function MAKE.DCS(state, host, t)
  local m = {}
  for name, read in pairs(DCS_READS) do
    m[name] = function()
      local v = host[read[1]]
      if v == nil then
        return read[2]
      end
      return v
    end
  end
  m.getSimulatorMode = function()
    if host.simulator_mode == nil then
      error("harness: " .. state .. ".DCS.getSimulatorMode has no modelled value; set host.simulator_mode", 2)
    end
    return host.simulator_mode
  end
  m.setUserCallbacks = function(callbacks)
    host.callbacks = callbacks
  end
  m.setPause = function(paused)
    host.paused = paused
  end
  m.getMissionResult = function()
    return host.mission_result or {}
  end
  m.getLogHistory = function()
    return host.log_history or {}
  end
  m.stopMission = function()
    host.stopped = true
  end
  m.exportToMiz = function(path)
    host.exported = path
  end
  return t.strict(state .. ".DCS", m)
end

-- `log.write` appends to `host.log`. The level constants are distinct
-- placeholders: the executor passes them through and never reads them.
function MAKE.log(state, host, t)
  return t.strict(state .. ".log", {
    ALL = 0,
    DEBUG = 1,
    INFO = 2,
    WARNING = 3,
    ERROR = 4,
    ALERT = 5,
    write = function(subsystem, level, message)
      host.log = host.log or {}
      host.log[#host.log + 1] = { state = state, subsystem = subsystem, level = level, message = message }
    end,
  })
end

--------------------------------------------------------------------------------

-- Build one state's globals. Every call is a fresh model: two suites, or two
-- loads of the executor in one suite, never share a table.
return function(t, name, host)
  local surface = SURFACE[name]
  if not surface then
    error("harness: no DCS state named " .. tostring(name) .. "; the states are " .. table.concat(STATES, ", "), 2)
  end
  local g = {}
  for _, k in ipairs(BASE) do
    g[k] = rawget(_G, k)
  end
  for _, k in ipairs(surface) do
    g[k] = MAKE[k](name, host, t)
  end
  local model = t.strict(name, g)
  model._G = model
  return model
end
