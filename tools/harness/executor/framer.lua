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
-- The reply carries `status`, `protocol: 2`, `host`, `stamp`, `phase`, `id`
-- and `tick` before the caller's headers, and a refused header writes
-- nothing. A
-- publish over an existing file replaces it, on the real filesystem, where
-- Windows refuses a rename onto a name that exists; one into a directory
-- that is not there refuses and makes nothing; one onto a file something
-- holds open refuses, leaves the old bytes, and leaves no `.tmp`. The hold
-- is the C runtime's, the kind `io.open` makes: a client's share-delete
-- hold lets the remove through and fails the rename instead, and ends the
-- same way.
--
-- Under a model of DCS's own `io` and `os`, whose calls answer a success
-- with nothing, a publish still lands, a take still gives the bytes back,
-- and a load and the launch after it still publish, rotate and sweep. A
-- write, a close or a rename that answers nothing and does nothing is
-- caught by the stat after it and leaves no file, as is a rename that did
-- nothing over a file of the same size; a remove that does the same
-- withholds the request's bytes; `false` and a message is still a refusal.
--
-- A request is taken off the disk: opened for bytes, read whole, and
-- removed before the bytes come back, so what it holds cannot run twice.
-- Its size comes from a stat, so a request over 262,144 bytes is removed
-- and refused `bad-request` with both numbers and no open of it in the
-- log; exactly the limit is taken. A request that is not there, or a
-- directory under a request's name, is `gone`; one that reads but cannot
-- be removed is `error` with its bytes withheld. `loadstring` is replaced
-- for the whole suite with one that fails a check: nothing here compiles
-- anything yet, so this is a guard for the dispatcher to come, not what
-- catches the size mutation, which the refusals and the absent open do.
--
-- The mutations this suite exists to catch. Escape a newline instead of
-- refusing it and the CR/LF case reads bytes where it wants nil. Frame from
-- a map with `pairs` and the order case fails on whichever run reorders it.
-- Write the final name directly and the log holds an open of it, no rename,
-- and the listing at the rename is never taken: the operation-log case goes
-- red on its first line. Drop the `os.remove` before the rename and the
-- rewrite-in-place case goes red on this host, where the rename refuses.
-- Leave the `.tmp` behind after a failed rename and the held case reads a
-- file where it wants nothing. Drop the size test in `take` and the 300 KiB
-- case reads bytes where it wants nil; read the file before testing its
-- size and the case reads an open in the log where it wants a remove
-- alone. Return the bytes before the remove and the held request case
-- reads them where it wants nil. Read a bare nil as a refusal, the way
-- Lua 5.1's answers allow, and the modelled publish refuses as the first
-- live load did; trust a silent write, close, rename or remove without
-- asking the disk, or ask it about the size alone, and its lying case
-- reads success.
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
  env.loadstring = function()
    t.check(false, "the executor compiled something: nothing under this suite may be parsed as code")
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
      .. "\ntick: 0\ncpu_ms: 0.000\nresult_type: string\n\n" .. body,
    "the reply is the session's headers, the caller's, the blank line, the body byte for byte")
end

do
  local E, _, _, log = spied("export")
  t.eq(E and E.reply("1-a", "error", nil, "no"), true, "export: a reply with no headers of the caller's is published")
  t.eq(slurp(E.res .. "\\1-a.res"),
    "status: error\nprotocol: 2\nhost: export\nstamp: " .. E.stamp .. "\nphase: loaded\nid: 1-a\ntick: 0\ncpu_ms: 0.000\n\nno",
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
  t.eq(entries(env, E.output), "events.log executor.txt", "rewrite: no .tmp is left")
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

--------------------------------------------------------------------------------
-- What DCS's own io and os answer
--------------------------------------------------------------------------------

-- DCS's library answers some successes with nothing at all where Lua 5.1's
-- answers true: the first live load refused its handshake with no reason,
-- which Lua 5.1 never gives. Which calls do is not measured, so this model
-- makes every one of them do it. `how` names what each of `write`,
-- `close`, `remove` and `rename` does: `quiet`, the default, does the work
-- and answers nothing on success and nil and a message on failure; `lie`
-- does nothing and answers nothing; `loud` answers the way Lua 5.1 does.
-- A handle buffers what is written and puts it on the disk at the close,
-- so a close that lies loses every byte the way an unflushed buffer would.
-- Installed over whatever the state already had, so over `spied` its log
-- still sees every call.
local function silenced(env, how)
  how = how or {}
  local open, remove, rename = env.io.open, env.os.remove, env.os.rename
  local function answer(call, ok, ...)
    if ok and (how[call] or "quiet") == "quiet" then
      return
    end
    return ok, ...
  end
  env.io.open = function(path, m)
    local fh, why = open(path, m)
    if not fh then
      return fh, why
    end
    local held = {}
    return {
      write = function(_, bytes)
        if how.write == "lie" then
          return
        end
        held[#held + 1] = bytes
        return answer("write", true)
      end,
      close = function()
        if how.close == "lie" then
          fh:close()
          return
        end
        local ok, failed = true, nil
        if #held > 0 then
          ok, failed = fh:write(table.concat(held))
        end
        local closed, why2 = fh:close()
        if ok then
          ok, failed = closed, why2
        end
        return answer("close", ok, failed)
      end,
      read = function(_, ...)
        return fh:read(...)
      end,
    }
  end
  env.os.remove = function(path)
    if how.remove == "lie" then
      return
    end
    return answer("remove", remove(path))
  end
  env.os.rename = function(from, to)
    if how.rename == "lie" then
      return
    end
    return answer("rename", rename(from, to))
  end
end

-- Lua 5.1's answers everywhere except the calls named.
local function only(calls)
  local how = { write = "loud", close = "loud", remove = "loud", rename = "loud" }
  for call, what in pairs(calls) do
    how[call] = what
  end
  return how
end

-- Every call answering nothing on success: a publish lands, and lands again
-- over itself.
do
  local E, env, _, log = spied("hook")
  silenced(env)
  local path = E.output .. "\\executor.txt"
  local ok, why = E.publish(path, "one")
  t.eq(ok, true, "dcs: a publish whose calls answer nothing on success lands: " .. tostring(why))
  t.eq(E.publish(path, "two"), true, "dcs: and lands again over itself")
  t.eq(slurp(path), "two", "dcs: the bytes are on the disk")
  t.eq(ops(log), published(path) .. "\n" .. published(path), "dcs: by the same sequence")
  t.eq(entries(env, E.output), "events.log executor.txt", "dcs: no .tmp is left")
end

-- One call answering nothing and doing nothing, every other answering the
-- way Lua 5.1 does: the stat after the rename catches each, and neither the
-- file that did not land nor a .tmp is left.
local function lands_not(call, what)
  local E, env = spied("hook")
  silenced(env, only({ [call] = "lie" }))
  local final = E.res .. "\\1-a.res"
  local ok, why = E.publish(final, "one")
  t.eq(ok, nil, "dcs " .. call .. ": " .. what .. " is refused")
  t.check(type(why) == "string" and why:find(final .. ": nothing refused, but the 3 bytes written did not land", 1, true),
    "dcs " .. call .. ": saying so: " .. tostring(why))
  t.eq(entries(env, E.res), "", "dcs " .. call .. ": and neither a file nor a .tmp is left")
end
lands_not("rename", "a rename that answered nothing and did nothing")
lands_not("write", "a write that answered nothing and wrote nothing")
lands_not("close", "a close that answered nothing and flushed nothing")

-- A rename that did nothing over a file already there at the same size,
-- with a remove that did nothing before it: the size agrees, and the .tmp
-- still standing is what says the new bytes did not land.
do
  local E, env = spied("hook")
  local final = E.res .. "\\1-a.res"
  t.eq(E.publish(final, "one"), true, "dcs stale: the first publish lands")
  silenced(env, only({ remove = "lie", rename = "lie" }))
  local ok, why = E.publish(final, "two")
  t.eq(ok, nil, "dcs stale: a rename that did nothing over a file of the same size is refused")
  t.check(type(why) == "string" and why:find("did not land", 1, true), "dcs stale: saying so: " .. tostring(why))
end

-- A host answering `false` and a message refuses, as `not ok` always read it.
do
  local E, env = spied("hook")
  env.os.rename = function()
    return false, "denied"
  end
  local final = E.res .. "\\1-a.res"
  local ok, why = E.publish(final, "one")
  t.eq(ok, nil, "dcs false: a rename answering false and a message is refused")
  t.eq(why, final .. ": denied", "dcs false: with its message")
  t.eq(entries(env, E.res), "", "dcs false: and no .tmp is left")
end

--------------------------------------------------------------------------------
-- A request, taken off the disk
--------------------------------------------------------------------------------

-- One request under `E.req`, written through the runner's own `io` so the
-- log holds nothing of it. Returns the path.
local function request(E, name, content)
  local path = E.req .. "\\" .. name
  local fh = assert(io.open(path, "wb"))
  fh:write(content)
  fh:close()
  return path
end

do
  local E, env, _, log = spied("hook")
  t.eq(type(E.take), "function", "the namespace carries take")
  t.eq(E.max_request_bytes, 262144, "and the request limit")
  local body = "for: " .. E.stamp .. "\r\nop: eval\r\n\r\nreturn '\0\255\128'\n"
  local path = request(E, "1-a.req", body)
  t.eq(E.take(path), body, "a request comes back byte for byte")
  t.eq(ops(log), "open rb " .. path .. "\nremove " .. path, "opened for bytes, then removed")
  t.eq(mode(env, path), nil, "the request is gone")
  t.eq(entries(env, E.req), "", "and req is empty")
end

-- The limit is a bound on the bytes: exactly 262,144 is taken and one more
-- is refused.
do
  local E, env, _, log = spied("hook")
  local at = request(E, "at.req", string.rep("x", 262144))
  local over = request(E, "over.req", string.rep("x", 262145))
  t.eq(#E.take(at), 262144, "limit: a request of exactly the limit is taken")
  for i = #log, 1, -1 do
    log[i] = nil
  end
  local bytes, status, why = E.take(over)
  t.eq(bytes, nil, "limit: one byte over is not")
  t.eq(status, "bad-request", "limit: it is bad-request")
  t.check(why:find("262145 bytes", 1, true) and why:find("262144-byte limit", 1, true),
    "limit: the message carries both numbers: " .. tostring(why))
  t.eq(ops(log), "remove " .. over, "limit: the file was removed and never opened")
  t.eq(entries(env, E.req), "", "limit: both are gone")
end

-- A 300 KiB request: refused without a byte of it read.
do
  local E, env, _, log = spied("hook")
  local code = "for: x\n\n" .. string.rep("x", 307200 - 8)
  t.eq(#code, 307200, "big: the request is 300 KiB")
  local big = request(E, "big.req", code)
  local bytes, status, why = E.take(big)
  t.eq(bytes, nil, "big: refused")
  t.eq(status, "bad-request", "big: as bad-request")
  t.check(why:find("307200 bytes", 1, true), "big: naming its size: " .. tostring(why))
  t.eq(ops(log), "remove " .. big, "big: never opened, the size came from a stat, and the file was removed")
  t.eq(mode(env, big), nil, "big: the file is gone")
end

-- A request that went between the listing and the take, and a directory
-- under a request's name: nothing to answer and nothing to answer to.
do
  local E, env, _, log = spied("hook")
  local bytes, status, why = E.take(E.req .. "\\vanished.req")
  t.eq(bytes, nil, "gone: a request that is not there is not taken")
  t.eq(status, "gone", "gone: and is gone, not bad")
  t.check(why:find("vanished.req", 1, true), "gone: named: " .. tostring(why))
  t.eq(#log, 0, "gone: nothing was opened or removed")
  assert(env.lfs.mkdir(E.req .. "\\dir.req"))
  bytes, status = E.take(E.req .. "\\dir.req")
  t.eq(bytes, nil, "dir: a directory under a request's name is not taken")
  t.eq(status, "gone", "dir: and is gone")
  t.eq(mode(env, E.req .. "\\dir.req"), "directory", "dir: and is left alone")
end

-- A request something holds open reads, because the hold shares reading,
-- and then cannot be removed: the bytes are withheld.
do
  local E, env, _, log = spied("hook")
  local path = request(E, "held.req", "return 1")
  local held = assert(io.open(path, "rb"))
  local bytes, status, why = E.take(path)
  held:close()
  t.eq(bytes, nil, "held: a request that cannot be removed is not returned")
  t.eq(status, "error", "held: it is an error")
  t.check(why:find("could not be removed", 1, true), "held: saying so: " .. tostring(why))
  t.eq(mode(env, path), "file", "held: the file stays")
  t.eq(ops(log), "open rb " .. path .. "\nremove " .. path, "held: it was read and the remove was tried")
end

-- A remove that answers nothing on success: the bytes come back. One that
-- answers nothing and removes nothing: they are withheld.
do
  local E, env = spied("hook")
  silenced(env)
  local path = request(E, "1-a.req", "return 1")
  local bytes, status, why = E.take(path)
  t.eq(bytes, "return 1", "dcs take: a remove that answered nothing on success gives the bytes back: "
    .. tostring(status) .. " " .. tostring(why))
  t.eq(mode(env, path), nil, "dcs take: and the request is gone")
end

do
  local E, env = spied("hook")
  silenced(env, { remove = "lie" })
  local path = request(E, "1-a.req", "return 1")
  local bytes, status, why = E.take(path)
  t.eq(bytes, nil, "dcs take lie: a remove that answered nothing and removed nothing withholds the bytes")
  t.eq(status, "error", "dcs take lie: it is an error")
  t.check(type(why) == "string" and why:find("could not be removed: it is still there", 1, true),
    "dcs take lie: saying so: " .. tostring(why))
  t.eq(mode(env, path), "file", "dcs take lie: the file stays")
end

-- A whole load over a host whose calls all answer nothing on success
-- publishes its handshake, which is what the first live load could not.
-- A second launch over it rotates the events log the first one wrote and
-- sweeps the first session, a request in it, with the same answers.
do
  local host = sandboxed({ pid = 7 })
  local env = t.state("hook", host)
  silenced(env)
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(type(E), "table", "dcs load: the namespace is published: " .. tostring(host.log and host.log[1] and host.log[1].message))
  t.eq(E and mode(env, E.handshake), "file", "dcs load: the handshake is on the disk")
  t.check(E and (slurp(E.handshake) or ""):find("protocol: 2", 1, true), "dcs load: and holds the envelope")
  t.eq(E and E.unrecorded, 0, "dcs load: the events line was written")
  request(E, "old.req", "return 1")
  host.pid = 8
  local again = t.state("hook", host)
  silenced(again)
  t.load_executor(again)()
  local second = rawget(again, NAME)
  t.eq(type(second), "table", "dcs relaunch: the namespace is published")
  t.eq(second and second.events_left, nil, "dcs relaunch: the events log was rotated: " .. tostring(second and second.events_left))
  t.eq(mode(again, second.output .. "\\events.prev.log"), "file", "dcs relaunch: into events.prev.log")
  t.eq(second.swept, 1, "dcs relaunch: the first session is swept")
  t.eq(second.sweep_left, nil, "dcs relaunch: and nothing of it is left")
  t.eq(mode(again, E.session), nil, "dcs relaunch: its directory is gone")
end
