-- The request parser: a request's envelope read back into headers and a
-- body, driven through the namespace the load publishes, over a sandbox
-- the way `executor/framer` does.
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
-- The mutations this suite exists to catch. Normalise the body along with
-- the headers and the byte-for-byte case reads changed bytes. Split at the
-- last colon and the first-colon case reads the tail of the value. Compare
-- names by case and the `FOR:` case finds no `for`. Find the blank line
-- with a search for two LFs and the whole-CRLF case reads no envelope.
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
  headers = E.parse("a:x\nb:   x\nc:\tx\nd: x \ne:\nf: \ng: a\tb\n\n")
  t.eq(headers and headers.a, "x", "no blank after the colon")
  t.eq(headers and headers.b, "x", "several blanks after the colon are dropped")
  t.eq(headers and headers.c, "x", "a tab after the colon is dropped")
  t.eq(headers and headers.d, "x ", "a trailing blank is kept")
  t.eq(headers and headers.e, "", "an empty value is read as empty")
  t.eq(headers and headers.f, "", "and so is one that is a blank")
  t.eq(headers and headers.g, "a\tb", "a tab inside a value is kept")
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
