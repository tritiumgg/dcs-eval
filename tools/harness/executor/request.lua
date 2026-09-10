-- The request parser: a request's envelope read back into headers and a
-- body, and a request admitted to run or answered `bad-request`, driven
-- through the namespace the load publishes, over a sandbox the way
-- `executor/framer` does.
--
-- What is proved. An envelope is header lines, one blank line, then the
-- body byte for byte. A name is read without regard to case and comes back
-- lowered; a value is everything after the first colon, so one may carry
-- colons of its own, with leading blanks dropped, trailing ones kept, and an
-- empty one allowed. A header block ending its lines with CRLF, or mixing
-- CRLF and LF, reads the same as one ending them with LF, and the body is
-- untouched whatever it holds: CRLF, NUL, bytes past ASCII, a line shaped
-- like a header, a leading blank line of its own. Refused, with the line
-- named: a line without a colon or with a space before it, a name outside
-- `[A-Za-z0-9_-]+`, a name repeated in another case, a value past ASCII or
-- carrying a CR, no bytes at all, and headers no blank line ends. The parse
-- touches no file.
--
-- A request is admitted from its path: taken off the disk, read, and
-- handed back as its id, headers and body with nothing written, the id
-- being the filename without `.req` and never checked against the shape of
-- one. It is answered `bad-request` under that name, with the file gone and
-- nothing handed back, when it names no `for` or an empty one, names no
-- op, is an `eval` with an empty body, does not parse, or is over the size
-- limit, in which case the log shows it was never opened. An `eval` whose
-- body is one blank or one newline is admitted with it, as is a `ping`
-- with any body or none. A `for` that is not the stamp is admitted here;
-- the stamp fence, when it comes, answers it `stale-session`. A request
-- that went before it could be taken is `gone` and nothing is written; one
-- read that cannot be removed is answered `error` with `stage: bridge`, the
-- wire's word for the executor's own failure, and its bytes withheld; a
-- reply that cannot be published comes back as `error` with the reason, so
-- a request already consumed is not lost in silence. `loadstring` and
-- `net.dostring_in` are replaced for the whole suite with ones that fail a
-- check: nothing here runs anything, so they are a guard for the dispatcher
-- to come, and a request refused here can never have reached one.
--
-- The mutations this suite exists to catch. Normalise the body along with
-- the headers and the byte-for-byte case reads changed bytes. Split at the
-- last colon and the first-colon case reads the tail of the value. Compare
-- names by case and the `FOR:` case finds no `for`. Find the blank line
-- with a search for two LFs and the whole-CRLF case reads no envelope.
-- Drop the `for` check in `admit` and the fence-precursor case reads a
-- request where it wants nil, with no reply on the disk. Admit an `eval`
-- with an empty body and the empty-body case reads a request where it
-- wants nil and finds no `status: bad-request` under `res`. Trim the body
-- before testing it and the one-blank body case reads nil where it wants a
-- request. Check the filename against the shape of an id and the first
-- case planted under a name that is not one, the oversize `big.req`, reads
-- gone where it wants bad-request.
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

local function parent(path)
  return path:match("^(.+)[\\/][^\\/]+$")
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

-- A state over a sandbox with spies on the calls a take and a publish make,
-- installed before the load so nothing the executor captured can be the
-- original, and the log emptied after it. Each entry is the call as a list
-- of words. `loadstring` and `net.dostring_in` are replaced with ones that
-- fail a check: nothing here runs anything, so a request that reached
-- either was run when it should have been refused. Returns the namespace,
-- the state, the sandbox and the log.
local function spied(state, host)
  local box
  host, box = sandboxed(host)
  local env = t.state(state or "hook", host)
  local log = {}
  local open, remove, rename = env.io.open, env.os.remove, env.os.rename
  env.io.open = function(path, m)
    log[#log + 1] = { "open", m, path }
    return open(path, m)
  end
  env.os.remove = function(path)
    log[#log + 1] = { "remove", path }
    return remove(path)
  end
  env.os.rename = function(from, to)
    log[#log + 1] = { "rename", from, to }
    return rename(from, to)
  end
  env.loadstring = function()
    t.check(false, "the executor compiled something: nothing under this suite may run")
  end
  if state ~= "export" then
    env.net.dostring_in = function()
      t.check(false, "the executor sent something to a state: nothing under this suite may run")
    end
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  for i = #log, 1, -1 do
    log[i] = nil
  end
  return E, env, box, log
end

-- One line per call in an operation log.
local function ops(log)
  local lines = {}
  for i, entry in ipairs(log) do
    lines[i] = table.concat(entry, " ")
  end
  return table.concat(lines, "\n")
end

-- The keys of a map, sorted and joined, so a whole map is one comparison.
local function keys(map)
  local names = {}
  for name in pairs(map) do
    names[#names + 1] = name
  end
  table.sort(names)
  return table.concat(names, " ")
end

--------------------------------------------------------------------------------
-- The envelope, read back
--------------------------------------------------------------------------------

do
  local E, _, _, log = spied("hook")
  t.eq(type(E), "table", "the namespace is published")
  t.eq(type(E.parse), "function", "and carries parse")

  local headers, body = E.parse("op: eval\nfor: 123-4\n\nreturn 1\n")
  t.eq(keys(headers or {}), "for op", "an LF block: the headers are read into a map")
  t.eq(headers.op, "eval", "op is read")
  t.eq(headers["for"], "123-4", "for is read")
  t.eq(body, "return 1\n", "the body is everything after the blank line")

  headers, body = E.parse("Op: ping\nFOR: x\nChunkName: =a\n\n")
  t.eq(headers and keys(headers), "chunkname for op", "names come back lowered whatever case they came in")
  t.eq(headers["for"], "x", "FOR is for")
  t.eq(headers.chunkname, "=a", "ChunkName is chunkname")
  t.eq(body, "", "no bytes after the blank line is an empty body")

  local chunk = "line\r\nnul\0x\255\128 for: fake\n\n\nend"
  headers, body = E.parse("for: x\n\n" .. chunk)
  t.eq(headers and keys(headers), "for", "a body shaped like more headers is not read as any")
  t.eq(body, chunk, "the body passes byte for byte: CRLF, NUL, bytes past ASCII, a header-shaped line, blank lines")

  headers, body = E.parse("op: eval\r\nfor: x\r\n\r\nreturn 'a\r\nb'")
  t.eq(headers and keys(headers), "for op", "a whole-CRLF block reads the same headers")
  t.eq(headers and headers["for"], "x", "with the CR gone from the value")
  t.eq(body, "return 'a\r\nb'", "and the body starts right after the CRLF pair, its own CRLF untouched")

  headers, body = E.parse("op: eval\r\nfor: x\n\r\n\n\nx")
  t.eq(headers and keys(headers), "for op", "a block mixing CRLF and LF reads the same headers")
  t.eq(body, "\n\nx", "and a body beginning with blank lines keeps them")

  headers = E.parse("\nanything")
  t.eq(headers and keys(headers), "", "a blank line first is an envelope with no headers")

  t.eq(#log, 0, "the parse touched no file")
end

-- The value: everything after the first colon, leading blanks dropped.
do
  local E = spied("hook")
  local headers = E.parse("chunkname: a:b:c\nx: C:\\y.lua\n\n")
  t.eq(headers and headers.chunkname, "a:b:c", "the first colon splits and the rest is the value")
  t.eq(headers and headers.x, "C:\\y.lua", "a drive letter survives")
  headers = E.parse("a:x\nb:   x\nc:\tx\nd: x \ne:\nf: \ng: a\tb\nh:\vx\ni:\fx\nj: \127\n\n")
  t.eq(headers and headers.a, "x", "no blank after the colon")
  t.eq(headers and headers.b, "x", "several blanks after the colon are dropped")
  t.eq(headers and headers.c, "x", "a tab after the colon is dropped")
  t.eq(headers and headers.d, "x ", "a trailing blank is kept")
  t.eq(headers and headers.e, "", "an empty value is read as empty")
  t.eq(headers and headers.f, "", "and so is one that is a blank")
  t.eq(headers and headers.g, "a\tb", "a tab inside a value is kept")
  t.eq(headers and headers.h, "x", "a vertical tab after the colon is dropped, as the framer refuses to write one")
  t.eq(headers and headers.i, "x", "and so is a form feed")
  t.eq(headers and headers.j, "\127", "DEL is the last byte of ASCII and is kept")
end

-- Refusals, each naming the line.
do
  local E = spied("hook")
  local function refused(bytes, pattern, what)
    local headers, why = E.parse(bytes)
    t.eq(headers, nil, what .. ": refused")
    t.check(type(why) == "string" and why:find(pattern, 1, true) ~= nil,
      what .. ": the reason says so: " .. tostring(why))
  end
  refused("for: x\nnocolon\n\n", "line 2 is not a header: nocolon", "a line without a colon")
  refused("for : x\n\n", "line 1 is not a header: for : x", "a blank before the colon")
  refused(" for: x\n\n", "line 1 is not a header", "a line beginning with a blank")
  refused("f\195\182r: x\n\n", "line 1 is not a header", "a name past ASCII")
  refused("a b: x\n\n", "line 1 is not a header", "a blank in a name")
  refused(": x\n\n", "line 1 is not a header", "an empty name")
  refused("for: x\nop: a\nFOR: y\n\n", "line 3: FOR: repeated", "a name repeated in another case")
  refused("for: caf\233\n\n", "line 1: for: the value is not ASCII", "a value past ASCII")
  refused("for: x\128\n\n", "line 1: for: the value is not ASCII", "the first byte past ASCII")
  refused("for: x\255\n\n", "line 1: for: the value is not ASCII", "the last byte")
  refused("for: a\rb\n\n", "line 1: for: the value carries a CR", "a CR inside a value")
  refused("for: x\n\r\r\n\n", "line 2 is not a header", "a line of one CR is not blank")
  refused("", "the headers never end", "no bytes")
  refused("for: x\n", "the headers never end", "headers with no blank line after them")
  refused("for: x\nop: eval", "after 1 header lines", "a last line without its LF, counted before it")
  refused("for: x\r\n", "the headers never end", "a CRLF-ended block with no blank line")
  refused("for: x\r\n\r", "the headers never end", "a lone CR where the blank line should be")
  refused(string.rep("x", 200) .. "\n\n", "x..." , "a long line is excerpted")
  local _, why = E.parse(string.rep("x", 200) .. "\n\n")
  t.eq(#why, #"line 1 is not a header: " + 83, "to 80 bytes and a mark")
end

--------------------------------------------------------------------------------
-- A request, admitted or refused
--------------------------------------------------------------------------------

-- The bytes of a file, through the runner's own `io` so the read leaves no
-- trace in the log. nil where there is no file.
local function slurp(path)
  local fh = io.open(path, "rb")
  if not fh then
    return nil
  end
  local bytes = fh:read("*a")
  fh:close()
  return bytes
end

-- One request under `E.req`, written through the runner's own `io` so the
-- log holds nothing of it. Returns the path.
local function request(E, name, content)
  local path = E.req .. "\\" .. name
  local fh = assert(io.open(path, "wb"))
  fh:write(content)
  fh:close()
  return path
end

-- The one sequence a take of `path` makes: opened for bytes, then removed.
local function taken(path)
  return "open rb " .. path .. "\nremove " .. path
end

-- The one sequence a publish of `path` makes.
local function published(path)
  return "open wb " .. path .. ".tmp\nremove " .. path .. "\nrename " .. path .. ".tmp " .. path
end

-- A reply on the disk under `id`: its status line, its `id` header, the
-- header `name` where one is asked for, and the body after the blank line.
local function answered(E, id, name)
  local bytes = slurp(E.res .. "\\" .. id .. ".res")
  if not bytes then
    return nil
  end
  local status = bytes:match("^status: ([^\n]*)\n")
  local echoed = bytes:match("\nid: ([^\n]*)\n")
  local extra = name and bytes:match("\n" .. name .. ": ([^\n]*)\n") or nil
  local body = bytes:match("\n\n(.*)$")
  return status, echoed, body, extra
end

-- A request that is refused: nil and the status come back, the reply is on
-- the disk under the planted name with that status and a body carrying
-- `pattern`, the request is gone, and the log is the take then the publish
-- and nothing else.
local function refused(E, env, log, name, content, pattern, what)
  for i = #log, 1, -1 do
    log[i] = nil
  end
  local path = request(E, name, content)
  local req, status, why = E.admit(path)
  t.eq(req, nil, what .. ": nothing is handed back")
  t.eq(status, "bad-request", what .. ": it is bad-request")
  t.check(type(why) == "string" and why:find(pattern, 1, true) ~= nil,
    what .. ": the message says so: " .. tostring(why))
  local id = name:match("^(.*)%.req$") or name
  local got, echoed, body = answered(E, id)
  t.eq(got, "bad-request", what .. ": the reply on the disk says bad-request")
  t.eq(echoed, id, what .. ": under the name the request came in")
  t.eq(body, why, what .. ": with the message as its body")
  t.eq(entries(env, E.req), "", what .. ": the request is gone")
  t.eq(ops(log), taken(path) .. "\n" .. published(E.res .. "\\" .. id .. ".res"),
    what .. ": it was taken, then the reply was published, and nothing else was touched")
  assert(os.remove(E.res .. "\\" .. id .. ".res"))
end

-- A request that is admitted: the record comes back, nothing is written,
-- and the log is the take alone.
local function admitted(E, env, log, name, content, what)
  for i = #log, 1, -1 do
    log[i] = nil
  end
  local path = request(E, name, content)
  local req, status, why = E.admit(path)
  t.eq(type(req), "table", what .. ": a request is handed back: " .. tostring(status) .. " " .. tostring(why))
  t.eq(entries(env, E.req), "", what .. ": the request is gone")
  t.eq(entries(env, E.res), "", what .. ": and nothing is written")
  t.eq(ops(log), taken(path), what .. ": the log is the take alone")
  return req
end

-- The well-formed request, and the fence precursor: one that names no
-- session is refused before anything could judge it.
do
  local E, env, _, log = spied("hook")
  t.eq(type(E.admit), "function", "the namespace carries admit")
  local chunk = "return '\0\255\128'\r\nfor: fake\n"
  local req = admitted(E, env, log, "0000000001-abcd.req", "op: eval\r\nFor: " .. E.stamp .. "\r\n\r\n" .. chunk, "eval")
  t.eq(req.id, "0000000001-abcd", "eval: the id is the filename without .req")
  t.eq(req.headers.op, "eval", "eval: the op is read")
  t.eq(req.headers["for"], E.stamp, "eval: for is read, lowered")
  t.eq(keys(req.headers), "for op", "eval: and nothing else was added")
  t.eq(req.body, chunk, "eval: the body is the chunk byte for byte")

  refused(E, env, log, "1-a.req", "op: eval\n\nreturn 1", "no for", "no for")
  refused(E, env, log, "1-b.req", "op: eval\nfor:\n\nreturn 1", "no for", "an empty for")
  refused(E, env, log, "1-c.req", "op: eval\nfor: \n\nreturn 1", "no for", "a blank for")

  req = admitted(E, env, log, "1-d.req", "op: eval\nfor: not-the-stamp\n\nreturn 1", "foreign")
  t.eq(req.headers["for"], "not-the-stamp",
    "foreign: a for that is not the stamp is admitted here, and the fence, when it comes, answers it stale-session")
end

-- The body: an eval with none is refused, one that is a blank or a newline
-- is not, and a ping is admitted with any body or none.
do
  local E, env, _, log = spied("hook")
  refused(E, env, log, "2-a.req", "op: eval\nfor: " .. E.stamp .. "\n\n", "the body is empty, and eval runs it", "empty body")
  refused(E, env, log, "2-b.req", "op: eval\r\nfor: " .. E.stamp .. "\r\n\r\n", "the body is empty", "empty body, CRLF")
  local req = admitted(E, env, log, "2-c.req", "op: eval\nfor: " .. E.stamp .. "\n\n ", "one blank")
  t.eq(req.body, " ", "one blank: the body is the blank, not trimmed away")
  req = admitted(E, env, log, "2-d.req", "op: eval\nfor: " .. E.stamp .. "\n\n\n", "one newline")
  t.eq(req.body, "\n", "one newline: the body is the newline")
  req = admitted(E, env, log, "2-e.req", "op: ping\nfor: " .. E.stamp .. "\n\n", "ping")
  t.eq(req.body, "", "ping: no body is admitted")
  req = admitted(E, env, log, "2-f.req", "op: ping\nfor: " .. E.stamp .. "\n\nignored", "ping with a body")
  t.eq(req.body, "ignored", "ping with a body: the body is kept, it is the op's to ignore")
  req = admitted(E, env, log, "2-g.req", "op: nope\nfor: " .. E.stamp .. "\n\n", "unknown op")
  t.eq(req.headers.op, "nope", "unknown op: admitted here, the dispatcher's to refuse")
end

-- The op, and an envelope that does not parse.
do
  local E, env, _, log = spied("hook")
  refused(E, env, log, "3-a.req", "for: " .. E.stamp .. "\n\nreturn 1", "no op", "no op")
  refused(E, env, log, "3-b.req", "for: " .. E.stamp .. "\nop:\n\nreturn 1", "no op", "an empty op")
  refused(E, env, log, "3-c.req", "for: " .. E.stamp .. "\nnocolon\n\nreturn 1", "line 2 is not a header: nocolon", "a line without a colon")
  refused(E, env, log, "3-d.req", "for: " .. E.stamp .. "\nop: eval\nreturn 1", "the headers never end", "no blank line")
  refused(E, env, log, "3-e.req", "", "the headers never end", "no bytes")
  refused(E, env, log, "3-f.req", "for: " .. E.stamp .. "\nchunkname: caf\233\n\nreturn 1", "chunkname: the value is not ASCII", "a value past ASCII")
end

-- Oversize: refused on the stat, never opened, and the reply carries both
-- numbers.
do
  local E, env, _, log = spied("hook")
  local path = request(E, "big.req", "op: eval\nfor: " .. E.stamp .. "\n\n" .. string.rep("x", 307200))
  local req, status, why = E.admit(path)
  t.eq(req, nil, "big: nothing is handed back")
  t.eq(status, "bad-request", "big: it is bad-request")
  local got, echoed, body = answered(E, "big")
  t.eq(got, "bad-request", "big: the reply says bad-request")
  t.eq(echoed, "big", "big: under its name")
  t.check(body and body:find("bytes, over the 262144-byte limit", 1, true), "big: naming the limit: " .. tostring(body))
  t.eq(body, why, "big: the message is the body")
  t.eq(ops(log), "remove " .. path .. "\n" .. published(E.res .. "\\big.res"), "big: removed unopened, then answered")
end

-- Gone: nothing to answer, and nothing written.
do
  local E, env, _, log = spied("hook")
  local req, status, why = E.admit(E.req .. "\\vanished.req")
  t.eq(req, nil, "gone: nothing is handed back")
  t.eq(status, "gone", "gone: it is gone")
  t.check(why and why:find("vanished.req", 1, true), "gone: named: " .. tostring(why))
  t.eq(entries(env, E.res), "", "gone: nothing is written")
  t.eq(#log, 0, "gone: and nothing was opened")
end

-- Held: read, not removable, answered error with its stage and the bytes
-- withheld.
do
  local E, env, _, log = spied("hook")
  local path = request(E, "held.req", "op: eval\nfor: " .. E.stamp .. "\n\nreturn 1")
  local held = assert(io.open(path, "rb"))
  local req, status, why = E.admit(path)
  held:close()
  t.eq(req, nil, "held: nothing is handed back")
  t.eq(status, "error", "held: it is error")
  local got, echoed, body, stage = answered(E, "held", "stage")
  t.eq(got, "error", "held: the reply says error")
  t.eq(stage, "bridge", "held: with stage bridge")
  t.eq(echoed, "held", "held: under its name")
  t.check(body and body:find("could not be removed", 1, true), "held: saying why: " .. tostring(body))
  t.eq(body, why, "held: the message is the body")
  t.eq(entries(env, E.req), "held.req", "held: the file stays")
  t.eq(ops(log), taken(path) .. "\n" .. published(E.res .. "\\held.res"), "held: the take was tried, then the reply published")
end

-- The name is only a filename: whatever it spells, the reply comes under
-- it, and nothing about it is checked.
do
  local E, env, _, log = spied("hook")
  refused(E, env, log, "weird name.req", "op: eval\n\nx", "no for", "a name with a blank")
  refused(E, env, log, "x.req.req", "op: eval\n\nx", "no for", "a name ending .req twice")
  local path = request(E, "noext", "op: eval\nfor: " .. E.stamp .. "\n\nx")
  local req = E.admit(path)
  t.eq(req and req.id, "noext", "a name without .req is the whole name")
  -- A name the framer cannot echo as the `id` header: the request is taken
  -- and refused, and the reply that would say so cannot be published, which
  -- comes back as the error it is rather than as a silent loss.
  path = request(E, "caf\233.req", "op: eval\n\nx")
  local status, why
  req, status, why = E.admit(path)
  t.eq(req, nil, "a name past ASCII: nothing is handed back")
  t.eq(status, "error", "a name past ASCII: it is error")
  t.check(why and why:find("reply to caf\233 was not published", 1, true) and why:find("id: the value is not ASCII", 1, true),
    "a name past ASCII: naming the reply and why the framer refused it: " .. tostring(why))
  t.eq(entries(env, E.req), "", "a name past ASCII: the request is gone")
  t.eq(entries(env, E.res), "", "a name past ASCII: and nothing was written")
end

-- A reply that cannot be published: the request is gone and the failure
-- comes back rather than nothing.
do
  local E, env, box = spied("hook")
  local path = request(E, "4-a.req", "op: eval\n\nx")
  E.res = box .. "\\nowhere"
  local req, status, why = E.admit(path)
  t.eq(req, nil, "unpublished: nothing is handed back")
  t.eq(status, "error", "unpublished: it is error")
  t.check(why and why:find("the bad-request reply to 4-a was not published", 1, true),
    "unpublished: naming the reply that was lost: " .. tostring(why))
  t.eq(entries(env, E.req), "", "unpublished: the request is gone all the same")
  -- The same with a request that could not be removed: the error reply is
  -- the one lost, and the file stays for the loop to see.
  path = request(E, "4-b.req", "op: eval\nfor: " .. E.stamp .. "\n\nx")
  local held = assert(io.open(path, "rb"))
  req, status, why = E.admit(path)
  held:close()
  t.eq(req, nil, "unpublished, held: nothing is handed back")
  t.eq(status, "error", "unpublished, held: it is error")
  t.check(why and why:find("the error reply to 4-b was not published", 1, true),
    "unpublished, held: naming the reply that was lost: " .. tostring(why))
  t.eq(entries(env, E.req), "4-b.req", "unpublished, held: the file stays")
end

-- The export state: the same path, under its own host and phase.
do
  local E, env, _, log = spied("export")
  local req = admitted(E, env, log, "5-a.req", "op: ping\nfor: " .. E.stamp .. "\n\n", "export")
  t.eq(req.id, "5-a", "export: the id")
  refused(E, env, log, "5-b.req", "op: ping\n\n", "no for", "export, no for")
  local path = request(E, "5-c.req", "op: ping\n\n")
  E.admit(path)
  local head = "status: bad-request\nprotocol: 2\nhost: export\nstamp: " .. E.stamp .. "\nphase: loaded\nid: 5-c\n\n"
  t.eq((slurp(E.res .. "\\5-c.res") or ""):sub(1, #head), head, "export: the reply names its host and phase")
end
