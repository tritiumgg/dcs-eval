-- The reply framer: the envelope's bytes, driven through the namespace the
-- load publishes, over a sandbox the way `executor/session` does.
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
-- The mutations this suite exists to catch. Escape a newline instead of
-- refusing it and the CR/LF case reads bytes where it wants nil. Frame from
-- a map with `pairs` and the order case fails on whichever run reorders it.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

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
