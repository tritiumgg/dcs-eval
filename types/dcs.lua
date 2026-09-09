---@meta
--
-- The DCS-provided globals the executor touches, declared so the language
-- server can check them instead of being told to ignore them.
--
-- This is deliberately not a catalogue of the DCS API. Cataloguing that surface
-- belongs to the consuming project, on its own records; what is here is the
-- host surface the executor and the client's read list actually name, and it
-- grows only when the build starts using something new.
--
-- Two of these are ED's additions to stock Lua and would otherwise read as
-- typos: `os.getpid` and `os.tmpdir` do not exist in 5.1.5 PUC-Rio, so the
-- reference interpreter has neither and the harness models both.
--
-- Never loaded at runtime. `---@meta` makes this a definition file.

--------------------------------------------------------------------------------
-- os: ED's additions to the stock library
--------------------------------------------------------------------------------

--- Process id of the running DCS process. ED's addition; absent in stock Lua.
--- Present in `hook`, `gui`, `scripting`, `config` and `export`, and the
--- session stamp is built from it.
---@return integer
function os.getpid() end

--- The system temporary directory, with a trailing separator. ED's addition.
---@return string
function os.tmpdir() end

--------------------------------------------------------------------------------
-- lfs: LuaFileSystem, plus the two directories ED adds
--------------------------------------------------------------------------------

---@class lfs
lfs = {}

--- Attributes of a file or directory, or nil plus a message when it cannot be
--- stat'd. The dormant path calls this and nothing else, once every few frames.
---@param path string
---@param request string|table|nil A single attribute name, or a table to fill
---@return table|string|number|nil
---@return string? error
function lfs.attributes(path, request) end

--- Iterator over the entries of a directory, `.` and `..` included.
---@param path string
---@return fun(): string?
function lfs.dir(path) end

--- The process's current working directory.
---@return string?
---@return string? error
function lfs.currentdir() end

--- Create one directory. Fails when the parent does not exist.
---@param path string
---@return boolean|nil
---@return string? error
function lfs.mkdir(path) end

--- Remove one empty directory. Fails when it has entries, or when a handle is
--- held on it, which on Windows is what a client watching it does.
---@param path string
---@return boolean|nil
---@return string? error
function lfs.rmdir(path) end

--- The user's writable DCS directory: `Saved Games\DCS*\`, with a trailing
--- separator. ED's addition. The transport root is chosen under it.
---@return string
function lfs.writedir() end

--- The DCS temporary directory, with a trailing separator. ED's addition.
---@return string
function lfs.tempdir() end

--------------------------------------------------------------------------------
-- log
--------------------------------------------------------------------------------

---@class log
---@field ALL integer
---@field DEBUG integer
---@field INFO integer
---@field WARNING integer
---@field ERROR integer
---@field ALERT integer
log = {}

--- Write one line to `dcs.log`. The load shell's last resort when it cannot
--- register: one line, then nothing.
---@param subsystem string
---@param level integer One of the log level fields
---@param message string
function log.write(subsystem, level, message) end

--------------------------------------------------------------------------------
-- net: present in `hook`, `gui` and `missionscripting`; nil in `export`
--------------------------------------------------------------------------------

---@class net
net = {}

--- Evaluate a chunk in another Lua state and return what it printed, as a
--- string. Three answers must be kept apart: a string is the result, nil is a
--- refusal by the policy gate, and an empty string is a successful evaluation
--- that returned nothing. Returns nil for every state on a client joined to a
--- real server.
---@param state string `'gui'`, `'scripting'`, `'mission'`, `'config'` or `'missionscripting'`
---@param code string
---@return string|nil
function net.dostring_in(state, code) end

--- The local player's id.
---@return integer
function net.get_my_player_id() end

--------------------------------------------------------------------------------
-- DCS: the hook host's table. nil in `export`.
--------------------------------------------------------------------------------

---@class DCS
DCS = {}

--- Register the callback table. The executor's whole hook-side entry point, and
--- the one call the load shell must survive raising.
---@param callbacks table
function DCS.setUserCallbacks(callbacks) end

---@return boolean
function DCS.getPause() end

---@param pause boolean
function DCS.setPause(pause) end

--- Seconds since the process started, monotonic across a mission load.
---@return number
function DCS.getRealTime() end

--- Seconds of simulated time. Does not advance while paused.
---@return number
function DCS.getModelTime() end

--- The simulator's mode. Its raw values per state are unmeasured; the live
--- stage records them rather than assuming a vocabulary.
---@return integer|string
function DCS.getSimulatorMode() end

---@return string
function DCS.getMissionName() end

---@return string
function DCS.getMissionFilename() end

---@return string
function DCS.getMissionDescription() end

---@return boolean
function DCS.getMissionLoaded() end

---@return table
function DCS.getMissionResult() end

---@return boolean
function DCS.isMultiplayer() end

---@return boolean
function DCS.isServer() end

---@return boolean
function DCS.isTrackPlaying() end

---@return table
function DCS.getLogHistory() end

function DCS.stopMission() end

---@param path string
function DCS.exportToMiz(path) end

--------------------------------------------------------------------------------
-- The mission-state door
--------------------------------------------------------------------------------

--- Run a chunk in the mission scripting environment, from the `mission` state.
--- One hop of the two the door takes; only callable with a mission loaded.
---@param code string
---@return any
function a_do_script(code) end
