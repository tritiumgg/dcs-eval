-- The reply framer: the envelope's bytes and how a reply reaches the disk,
-- driven through the namespace the load publishes, over a sandbox the way
-- `executor/session` does.
--
-- What is proved. An envelope is header lines, one blank line, then the
-- body byte for byte: headers in the order given, a number spelt with
-- `tostring`, an empty or absent body leaving the blank line as the end,
-- and a body carrying CRLF, NUL, bytes past ASCII and a line shaped like a
-- header passing untouched. A header is refused, never escaped: a value
-- with CR or LF, a byte past ASCII, or leading whitespace; a value that is
-- not a string or a number; a name outside `[A-Za-z0-9_-]+` or repeated; a
-- body that is not a string. A tab inside a value is ASCII and is kept.
--
-- A reply is published by rename: the bytes go to `<id>.res.tmp` in the
-- `res` directory, the final name is removed, then the `.tmp` is renamed
-- onto it. That is read off an operation log, because the directory looks
-- the same afterwards whichever way the file got there; the rename spy is
-- also where the directory is listed mid-publish, the moment a collector
-- reading nothing but `.res` would find nothing, which the frozen
-- specification says a single-process control cannot see and a spy can.
-- The reply carries `status`, `protocol: 2`, `host`, `stamp`, `phase` and
-- `id` before the caller's headers, and a refused header writes nothing. A
-- publish over an existing file replaces it, on the real filesystem, where
-- Windows refuses a rename onto a name that exists; one into a directory
-- that is not there refuses and makes nothing; one onto a file something
-- holds open refuses, leaves the old bytes, and leaves no `.tmp`. The hold
-- is the C runtime's, the kind `io.open` makes: a client's share-delete
-- hold lets the remove through and fails the rename instead, and ends the
-- same way.
--
-- The mutations this suite exists to catch. Escape a newline instead of
-- refusing it and the CR/LF case reads bytes where it wants nil. Frame from
-- a map with `pairs` and the order case fails on whichever run reorders it.
-- Write the final name directly and the log holds an open of it, no rename,
-- and the listing at the rename is never taken: the operation-log case goes
-- red on its first line. Drop the `os.remove` before the rename and the
-- rewrite-in-place case goes red on this host, where the rename refuses.
-- Leave the `.tmp` behind after a failed rename and the held case reads a
-- file where it wants nothing.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

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

local function parent(path)
  return path:match("^(.+)[\\/][^\\/]+$")
end

-- The bytes of a file, through the runner's own `io` so the read leaves no
-- trace in a log of the executor's calls. nil where there is no file.
local function slurp(path)
  local fh = io.open(path, "rb")
  if not fh then
    return nil
  end
  local bytes = fh:read("*a")
  fh:close()
  return bytes
end

-- One line per call in an operation log: the call, its mode where it has
-- one, and its paths.
local function ops(log)
  local lines = {}
  for i, entry in ipairs(log) do
    lines[i] = table.concat(entry.call, " ")
  end
  return table.concat(lines, "\n")
end

-- A host whose directories are under a fresh sandbox, and the sandbox.
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

-- Build one state over `host`, run the executor, and return the namespace
-- it published, or nil, and the state.
local function load(host, state)
  local env = t.state(state or "hook", host)
  t.load_executor(env)()
  return rawget(env, NAME), env
end

-- A state over a sandbox with spies on the calls a publish makes,
-- installed before the load so nothing the executor captured can be the
-- original, and the log emptied after it: what the load itself does to the
-- disk is `executor/session`'s to prove. Each entry is the call as a list
-- of words, and the rename's carries `listing`, the destination directory
-- as it stood when the rename was asked for. Returns the namespace, the
-- state, the sandbox and the log.
local function spied(state, host)
  local box
  host, box = sandboxed(host)
  local env = t.state(state or "hook", host)
  local log = {}
  local open, remove, rename = env.io.open, env.os.remove, env.os.rename
  env.io.open = function(path, m)
    log[#log + 1] = { call = { "open", m, path } }
    return open(path, m)
  end
  env.os.remove = function(path)
    log[#log + 1] = { call = { "remove", path } }
    return remove(path)
  end
  env.os.rename = function(from, to)
    log[#log + 1] = { call = { "rename", from, to }, listing = entries(env, parent(to)) }
    return rename(from, to)
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  for i = #log, 1, -1 do
    log[i] = nil
  end
  return E, env, box, log
end

-- The one sequence a publish of `path` may make.
local function published(path)
  return "open wb " .. path .. ".tmp\nremove " .. path .. "\nrename " .. path .. ".tmp " .. path
end

--------------------------------------------------------------------------------
-- The envelope
--------------------------------------------------------------------------------

do
  local E = load((sandboxed()))
  t.eq(type(E), "table", "the namespace is published")
  t.eq(type(E.frame), "function", "and carries frame")
  t.eq(E.frame({ { "status", "ok" } }, "hello"), "status: ok\n\nhello", "one header, the blank line, the body")
  t.eq(E.frame({ { "status", "ok" }, { "id", "1-a" }, { "tick", 7 } }, ""), "status: ok\nid: 1-a\ntick: 7\n\n",
    "headers keep their order, a number is spelt with tostring, an empty body ends at the blank line")
  t.eq(E.frame({ { "a", "b" } }, nil), "a: b\n\n", "an absent body is an empty one")
  t.eq(E.frame({}, "x"), "\nx", "no headers is the blank line and the body")
  local body = "line\r\nnul\0x\255\128 status: fake\n"
  t.eq(E.frame({ { "status", "ok" } }, body), "status: ok\n\n" .. body,
    "the body passes verbatim: CRLF, NUL, bytes past ASCII, a header-shaped line")
  t.eq(E.frame({ { "chunkname", "a\tb" } }, ""), "chunkname: a\tb\n\n", "a tab inside a value is ASCII and is kept")
  t.eq(E.frame({ { "Status", "ok" }, { "cpu_ms", 0.5 }, { "x-y", "1" } }, ""), "Status: ok\ncpu_ms: 0.5\nx-y: 1\n\n",
    "a name may carry case, an underscore and a dash")

  local function refused(headers, body, pattern, what)
    local bytes, why = E.frame(headers, body)
    t.eq(bytes, nil, what .. ": refused")
    t.check(type(why) == "string" and why:find(pattern, 1, true) ~= nil,
      what .. ": the reason says so: " .. tostring(why))
  end
  refused({ { "x", "a\nb" } }, "", "x: the value carries a CR or LF", "LF in a value")
  refused({ { "x", "a\rb" } }, "", "x: the value carries a CR or LF", "CR in a value")
  refused({ { "x", "a\r\n" } }, "", "x: the value carries", "CRLF at the end of a value")
  refused({ { "x", "caf\233" } }, "", "x: the value is not ASCII", "a byte past ASCII")
  refused({ { "x", " a" } }, "", "x: the value begins with whitespace", "a leading space")
  refused({ { "x", "\ta" } }, "", "x: the value begins with whitespace", "a leading tab")
  refused({ { "x", {} } }, "", "x: the value is a table", "a table value")
  refused({ { "x", true } }, "", "x: the value is a boolean", "a boolean value")
  refused({ { "x" } }, "", "x: the value is a nil", "a missing value")
  refused({ { "", "a" } }, "", "header 1: the name", "an empty name")
  refused({ { "a b", "a" } }, "", "header 1: the name", "a space in a name")
  refused({ { "a:b", "a" } }, "", "header 1: the name", "a colon in a name")
  refused({ { "ok", "1" }, { 7, "a" } }, "", "header 2: the name", "a number for a name, counted where it sits")
  refused({ { "id", "1" }, { "ID", "2" } }, "", "ID: repeated", "a name repeated in another case")
  refused({ { "status", "ok" } }, {}, "the body is a table", "a table body")
  refused({ { "status", "ok" } }, 7, "the body is a number", "a number body")
end

--------------------------------------------------------------------------------
-- A reply, published by rename
--------------------------------------------------------------------------------

do
  local E, env, _, log = spied("hook")
  t.eq(type(E) == "table" and type(E.reply), "function", "the namespace carries reply")
  t.eq(type(E.publish), "function", "and publish")
  local body = "\208\191\209\128\0\255 caf\233\r\nstatus: fake\n"
  local id = "0000000001-abcd"
  local final = E.res .. "\\" .. id .. ".res"
  t.eq(E.reply(id, "ok", { { "result_type", "string" } }, body), true, "a reply is published")
  t.eq(ops(log), published(final),
    "the publish is one open of the .tmp for writing, a remove of the final name and a rename onto it, and nothing else")
  t.eq(parent(final .. ".tmp"), E.res, "the .tmp is in the directory the reply lives in")
  t.eq(log[3].listing, id .. ".res.tmp",
    "at the rename the directory holds the .tmp and not the reply: a collector reading .res finds nothing yet")
  t.eq(entries(env, E.res), id .. ".res", "after it the directory holds the reply and no .tmp")
  t.eq(slurp(final),
    "status: ok\nprotocol: 2\nhost: hook\nstamp: " .. E.stamp .. "\nphase: menu\nid: " .. id
      .. "\nresult_type: string\n\n" .. body,
    "the reply is the session's headers, the caller's, the blank line, the body byte for byte")
end

do
  local E, _, _, log = spied("export")
  t.eq(E and E.reply("1-a", "error", nil, "no"), true, "export: a reply with no headers of the caller's is published")
  t.eq(slurp(E.res .. "\\1-a.res"),
    "status: error\nprotocol: 2\nhost: export\nstamp: " .. E.stamp .. "\nphase: loaded\nid: 1-a\n\nno",
    "export: the reply names its host and phase")
  t.eq(ops(log), published(E.res .. "\\1-a.res"), "export: the same sequence")
end

-- The name a request came under is only a filename to the executor.
do
  local E, env = spied("hook")
  t.eq(E.reply("not an id", "ok", {}, ""), true, "a name that is not an id is answered under it anyway")
  t.eq(entries(env, E.res), "not an id.res", "as the filename it spells")
end

-- A header the framer refuses stops the reply before the disk.
do
  local E, env, _, log = spied("hook")
  local ok, why = E.reply("1-a", "ok", { { "x", "a\nb" } }, "")
  t.eq(ok, nil, "a reply carrying a header the framer refuses is refused")
  t.check(why:find("x: the value carries", 1, true), "with the framer's reason: " .. tostring(why))
  t.eq(entries(env, E.res), "", "and nothing is written, not even a .tmp")
  t.eq(#log, 0, "and nothing was opened")
  ok, why = E.reply("1-a", "ok", { { "status", "again" } }, "")
  t.eq(ok, nil, "a caller's header repeating a session header is refused")
  t.check(why:find("status: repeated", 1, true), "as repeated: " .. tostring(why))
end

-- Rewriting a file in place, which the handshake and the heartbeat do. On
-- this host a rename onto a name that exists refuses, so the second publish
-- lands only because the remove came first.
do
  local E, env, _, log = spied("hook")
  local path = E.output .. "\\executor.txt"
  t.eq(E.publish(path, "one"), true, "rewrite: a file is published")
  t.eq(E.publish(path, "two"), true, "rewrite: and published again over itself")
  t.eq(slurp(path), "two", "rewrite: the second publish wins")
  t.eq(ops(log), published(path) .. "\n" .. published(path),
    "rewrite: each publish removes the final name before the rename")
  t.eq(entries(env, E.output), "executor.txt", "rewrite: no .tmp is left")
end

-- A directory that is not there: the open refuses and nothing is made.
do
  local E, env, box = spied("hook")
  local ok, why = E.publish(box .. "\\nowhere\\x.res", "x")
  t.eq(ok, nil, "nowhere: a publish into a directory that is not there refuses")
  t.check(why:find(box .. "\\nowhere\\x.res.tmp", 1, true), "nowhere: naming the .tmp it could not open: " .. tostring(why))
  t.eq(mode(env, box .. "\\nowhere"), nil, "nowhere: and makes nothing")
end

-- A file something holds open, the way the C runtime holds one: the remove
-- refuses, the rename refuses, the old bytes stay and the .tmp goes.
do
  local E, env, _, log = spied("hook")
  local final = E.res .. "\\1-a.res"
  t.eq(E.publish(final, "first"), true, "held: the first publish lands")
  local held = assert(io.open(final, "rb"))
  for i = #log, 1, -1 do
    log[i] = nil
  end
  local ok, why = E.publish(final, "second")
  held:close()
  t.eq(ok, nil, "held: a publish onto a file something holds open refuses")
  t.check(why:find(final, 1, true), "held: naming the file: " .. tostring(why))
  t.eq(slurp(final), "first", "held: the old bytes stay")
  t.eq(entries(env, E.res), "1-a.res", "held: and no .tmp survives")
  t.eq(ops(log), published(final) .. "\nremove " .. final .. ".tmp",
    "held: the remove and the rename were tried, then the .tmp was removed")
end
