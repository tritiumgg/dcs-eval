-- The `eval` op through the `net.dostring_in` carrier: the body carried
-- into another state as a `%q` literal inside a wrapper, compiled there
-- under the request's `chunkname`, run in that state's globals, converted
-- there, and sent back as one string the executor decodes; driven over a
-- sandbox the way `executor/eval-hook` drives the local carrier.
--
-- The model's `net.dostring_in` evaluates nothing, on purpose, so this
-- suite installs its own carrier over it: one that records what the
-- executor handed it, compiles the wrapper with the runner's own
-- `loadstring`, runs it with a modelled target state as its globals, and
-- returns what it returned. The wrapper then really runs, under the
-- reference interpreter, in a state with the target's surface and none
-- of the hook's; what it cannot prove is what DCS does with the wrapper,
-- which only a live install can, and the model's own stub stays
-- non-evaluating, pinned at the end.
--
-- What is proved. The three answers are kept apart: a string in the
-- wrapper's shape is the reply, `nil` is `refused`, the literal `Invalid
-- state name` is `invalid-state`, and none of the three is mistaken for
-- another. A string not in the wrapper's shape, the model's own empty
-- answer among them, and a value that is not a string are `error` under
-- `stage: dostring_in`. Through the wrapper a value comes back typed and
-- printed as the local carrier prints it, a number widened so it reads
-- back, and a table as an empty body; line 47 is true under every
-- spelling of `chunkname`, a compile error names it, CRLF counts the
-- same, and a long name is abbreviated in the message and whole in the
-- header. The chunk runs in the target state's globals, not the hook's:
-- a global it sets lands there, the hook's namespace is not visible, and
-- a sanitised state serves. The body crosses as a `%q` literal and
-- nothing of it is concatenated into the wrapper, a NUL before a digit
-- included. The ceiling is applied in the state, so what crosses back for
-- an oversize result is the refusal and not the result. `server` reaches
-- the carrier under its own name. A host without `net.dostring_in` is
-- `unsupported`, and the export host refuses every state but its own.
-- `missionscripting`, through `a_do_script`, is `executor/a_do_script`'s.
--
-- The mutations this suite exists to catch. Read a `nil` answer as an
-- empty `ok` and the three-answers checks read `ok` for `refused`. Read
-- the literal as a result and `invalid-state` reads `ok` with the literal
-- as its body. Prepend one line to the body inside the wrapper and every
-- `:47:` reads `:48:`. Drop `setfenv` from the wrapper and the global a
-- chunk sets lands in the runner's globals, where the sealed table
-- raises. Apply the ceiling after the crossing instead of in the state
-- and the string the carrier returned is over the ceiling. Pass
-- `scripting` for `server` and the recorded state differs.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The reply's headers, in order. The suite's own copy, kept apart from the
-- executor's on purpose. HEAD is a refusal; OK and ERR are an `eval` that
-- was compiled; OVERSIZE a result refused.
local HEAD = { "status", "protocol", "host", "stamp", "phase", "id", "tick" }
local OK = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "result_type", "chunkname" }
local ERR = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage", "chunkname" }
local OVERSIZE = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage", "chunkname", "result_bytes" }

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

-- A state over a fresh sandbox with the executor loaded into it. Returns
-- the namespace, the state, the host and the frame callback.
local function loaded(state)
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP }
  local env = t.state(state, host)
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(type(E), "table", "the executor loaded over " .. state)
  local frame
  if state == "hook" then
    frame = host.callbacks.onSimulationFrame
  else
    frame = rawget(env, "LuaExportAfterNextFrame")
  end
  return E, env, host, frame
end

-- The one place the model's stub is replaced, so a suite that installs a
-- carrier of its own does it as the executor will meet it: as the field
-- `net.dostring_in` of the hook state's `net`.
local function answering(env, fn)
  env.net.dostring_in = fn
end

-- The suite's carrier in place of the model's stub. `targets` maps a
-- state name to the model the wrapper runs in; a name not in it is what
-- DCS does not know. Each call is recorded in `seen`: the state, the
-- chunk handed over, and what came back.
local function carrier(env, targets, seen)
  answering(env, function(state, chunk)
    local record = { state = state, chunk = chunk }
    seen[#seen + 1] = record
    local target = targets[state]
    if not target then
      record.answered = "Invalid state name"
      return "Invalid state name"
    end
    local fn = assert(loadstring(chunk, "=wrapper"))
    setfenv(fn, target)
    record.answered = fn()
    return record.answered
  end)
end

-- One request under `E.req`, written through the runner's own `io`.
local function request(E, name, content)
  local fh = assert(io.open(E.req .. "\\" .. name, "wb"))
  fh:write(content)
  fh:close()
end

-- A reply read back with the suite's own reader: the names in the order
-- written, the values by name, and the body.
local function read(E, id)
  local fh = assert(io.open(E.res .. "\\" .. id .. ".res", "rb"), id .. ": no reply on the disk")
  local bytes = fh:read("*a")
  fh:close()
  local blank = bytes:find("\n\n", 1, true)
  t.check(blank, id .. ": the reply has a blank line ending its headers")
  local order, values = {}, {}
  for line in bytes:sub(1, blank):gmatch("([^\n]*)\n") do
    local name, value = line:match("^([A-Za-z0-9_%-]+): (.*)$")
    t.check(name, id .. ": every header line reads name: value, but one reads " .. line)
    order[#order + 1] = name
    values[name] = value
  end
  return order, values, bytes:sub(blank + 2)
end

-- The names of `order` against `want`, one line, so a missing or extra
-- header reads as which.
local function fields(order, want, what)
  local w, got = table.concat(want, " "), table.concat(order, " ")
  t.eq(#order, #want, what .. ": every header is present, and no other: " .. got)
  t.eq(got, w, what .. ": and in the wire's order")
end

-- An `eval` for the session under `id`, with `extra` header lines after
-- `op` and `for`, answered on one frame and read back.
local function eval(E, frame, id, extra, body)
  request(E, id .. ".req", "op: eval\nfor: " .. E.stamp .. "\n" .. (extra or "") .. "\n" .. body)
  frame()
  return read(E, id)
end

-- A body of 46 comment lines and `line` as its line 47, LF endings.
local function at47(line)
  local lines = {}
  for i = 1, 46 do
    lines[i] = "-- line " .. i
  end
  lines[47] = line
  return table.concat(lines, "\n")
end

-- The handshake's value for one header, read with the suite's own reader.
local function published(E, name)
  local fh = assert(io.open(E.handshake, "rb"))
  local bytes = fh:read("*a")
  fh:close()
  return bytes:match("\n" .. name .. ": ([^\n]*)\n")
end

--------------------------------------------------------------------------------
-- The three answers are kept apart
--------------------------------------------------------------------------------

do
  local E, env, host, frame = loaded("hook")
  local seen = {}
  local gui = t.state("gui", host)
  carrier(env, { gui = gui }, seen)

  local _
  local order, v, body = eval(E, frame, "1-a", "state: gui\n", 'return "crossed"')
  fields(order, OK, "string")
  t.eq(v.status, "ok", "string: a string in the wrapper's shape is the reply")
  t.eq(v.result_type, "string", "string: typed")
  t.eq(v.chunkname, "=dcs-eval", "string: the default chunkname is echoed")
  t.eq(body, "crossed", "string: the bytes verbatim")
  t.eq(#seen, 1, "string: the carrier was called once")
  t.eq(seen[1].state, "gui", "string: for the state the request named")
  t.eq(seen[1].answered, "ok\nstring\ncrossed", "string: and what crossed back was the three fields")

  -- A refusal: nil from the carrier, whatever the chunk would have done.
  answering(env, function(state, _)
    seen[#seen + 1] = { state = state }
    return nil
  end)
  order, v, body = eval(E, frame, "1-b", "state: gui\n", 'return "crossed"')
  fields(order, HEAD, "refused")
  t.eq(v.status, "refused", "refused: nil is refused, never an empty ok")
  t.check(body:find("^net%.dostring_in returned nil for gui: "), "refused: naming the state: " .. body)
  t.check(body:find("policy gate", 1, true), "refused: and the gate among the causes")
  t.eq(#seen, 2, "refused: the carrier was asked")

  -- The literal: the state is known and not reachable now.
  answering(env, function()
    return "Invalid state name"
  end)
  order, v, body = eval(E, frame, "1-c", "state: export\n", 'return "crossed"')
  fields(order, HEAD, "invalid")
  t.eq(v.status, "invalid-state", "invalid: the literal is invalid-state, never a string result")
  t.check(body:find("^net%.dostring_in answered 'Invalid state name' for export: "), "invalid: naming the state: " .. body)

  -- The literal as a chunk's own result is a result, because it arrives
  -- inside the wrapper's shape and not bare.
  carrier(env, { gui = gui }, seen)
  order, v, body = eval(E, frame, "1-d", "state: gui\n", 'return "Invalid state name"')
  fields(order, OK, "literal result")
  t.eq(v.status, "ok", "literal result: a chunk that returns the literal is ok")
  t.eq(body, "Invalid state name", "literal result: with the literal as its body")

  -- A nil a chunk returns is a typed nil, told from a refusal by the shape.
  order, v, body = eval(E, frame, "1-e", "state: gui\n", "return nil")
  fields(order, OK, "nil result")
  t.eq(v.status, "ok", "nil result: ok")
  t.eq(v.result_type, "nil", "nil result: typed nil")
  t.eq(body, "", "nil result: an empty body")

  -- A string not in the wrapper's shape: the state answered something
  -- other than the wrapper's reply.
  answering(env, function()
    return "something else"
  end)
  order, v, body = eval(E, frame, "1-f", "state: gui\n", "return 1")
  fields(order, ERR, "unshaped")
  t.eq(v.status, "error", "unshaped: a string not in the wrapper's shape is error")
  t.eq(v.stage, "dostring_in", "unshaped: under stage dostring_in")
  t.eq(body, "something else", "unshaped: with the string as the body")

  answering(env, function()
    return "ok\nstring"
  end)
  _, v, body = eval(E, frame, "1-g", "state: gui\n", "return 1")
  t.eq(v.stage, "dostring_in", "two fields: one line short of the shape is not the shape")
  t.eq(body, "ok\nstring", "two fields: carried as it came")

  answering(env, function()
    return "nope\n\nbody"
  end)
  _, v, body = eval(E, frame, "1-h", "state: gui\n", "return 1")
  t.eq(v.stage, "dostring_in", "unknown status: a word the wrapper never sends is not the shape")
  t.eq(body, "nope\n\nbody", "unknown status: carried whole")

  answering(env, function()
    return ""
  end)
  order, v, body = eval(E, frame, "1-i", "state: gui\n", "return 1")
  fields(order, ERR, "empty")
  t.eq(v.status, "error", "empty: an empty answer is not an empty ok")
  t.eq(v.stage, "dostring_in", "empty: under stage dostring_in")
  t.eq(body, "", "empty: with nothing as the body")

  answering(env, function()
    return {}
  end)
  order, v, body = eval(E, frame, "1-j", "state: gui\n", "return 1")
  fields(order, ERR, "table")
  t.eq(v.stage, "dostring_in", "table: a value that is not a string is error under dostring_in")
  t.eq(body, "net.dostring_in answered a table value for gui, and the executor reads a string alone",
    "table: named by type")

  answering(env, function()
    return setmetatable({}, { __tostring = function() return "ran" end })
  end)
  _, v, body = eval(E, frame, "1-k", "state: gui\n", "return 1")
  t.check(body:find("a table value", 1, true), "tostring: and never stringified: " .. body)

  t.eq(entries(env, E.req), "", "answers: every request is taken")
  t.eq(E.raised, 0, "answers: nothing reached the guard")
  t.eq(E.unpublished, 0, "answers: every reply was published")
end

--------------------------------------------------------------------------------
-- What comes back through the wrapper, by type
--------------------------------------------------------------------------------

do
  local E, env, host, frame = loaded("hook")
  local seen = {}
  carrier(env, { gui = t.state("gui", host) }, seen)

  local _, v, body = eval(E, frame, "2-a", "state: gui\n", "return 42")
  t.eq(v.result_type, "number", "number: typed")
  t.eq(body, "42", "number: printed")
  _, v, body = eval(E, frame, "2-b", "state: gui\n", "return 0.1 + 0.2")
  t.eq(body, "0.30000000000000004", "float: widened in the state so it reads back")
  _, v, body = eval(E, frame, "2-c", "state: gui\n", "return 0 / 0")
  t.eq(body, "nan", "nan: named in the state")
  _, v, body = eval(E, frame, "2-d", "state: gui\n", "return -1 / 0")
  t.eq(body, "-inf", "-inf: named in the state")
  _, v, body = eval(E, frame, "2-e", "state: gui\n", "return true")
  t.eq(v.result_type, "boolean", "boolean: typed")
  t.eq(body, "true", "boolean: named")
  _, v, body = eval(E, frame, "2-f", "state: gui\n", "return false")
  t.eq(body, "false", "false: named, not an empty body")
  _, v, body = eval(E, frame, "2-g", "state: gui\n", 'return ""')
  t.eq(v.result_type, "string", "empty string: typed as a string")
  t.eq(body, "", "empty string: an empty body, told from nil by the header")
  _, v, body = eval(E, frame, "2-h", "state: gui\n", "local x = 1")
  t.eq(v.result_type, "nil", "no return: nil")
  _, v, body = eval(E, frame, "2-i", "state: gui\n", "return { 1, 2 }")
  t.eq(v.status, "ok", "table: ok")
  t.eq(v.result_type, "table", "table: typed")
  t.eq(body, "", "table: converted in the state to an empty body, so no table crosses")
  t.eq(seen[#seen].answered, "ok\ntable\n", "table: and what crossed was the three fields with nothing after")
  _, v, body = eval(E, frame, "2-j", "state: gui\n", "return type")
  t.eq(v.result_type, "function", "function: typed")
  _, v, body = eval(E, frame, "2-k", "state: gui\n", "return 1, 2")
  t.eq(body, "1", "two values: only the first")
  _, v, body = eval(E, frame, "2-l", "state: gui\n", 'return "a\\0b\\nc\\r\\nd"')
  t.eq(body, "a\0b\nc\r\nd", "bytes: a NUL, a newline and a CRLF in a result cross verbatim")
  _, v, body = eval(E, frame, "2-m", "state: gui\n", 'return "one\\n\\ntwo\\n"')
  t.eq(v.result_type, "string", "newlines: a result with blank lines is typed")
  t.eq(body, "one\n\ntwo\n", "newlines: and carried whole, because the body is the rest of the string")
  _, v, body = eval(E, frame, "2-n", "state: gui\n",
    "return setmetatable({}, { __tostring = function() dostring_tostring_ran = true; return 'ran' end })")
  t.eq(body, "", "__tostring: an empty body")
  t.eq(rawget(env, "dostring_tostring_ran"), nil, "__tostring: and the metamethod never ran")

  t.eq(E.raised, 0, "types: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- Line numbers are true through the wrapper
--------------------------------------------------------------------------------

do
  local E, env, host, frame = loaded("hook")
  local seen = {}
  carrier(env, { gui = t.state("gui", host), mission = t.state("mission", host) }, seen)
  local boom = at47('error("boom")')

  local _
  local order, v, body = eval(E, frame, "3-a", "state: gui\nchunkname: @x.lua\n", boom)
  fields(order, ERR, "at")
  t.eq(v.status, "error", "at: a raise is error")
  t.eq(v.stage, "run", "at: while running")
  t.eq(v.chunkname, "@x.lua", "at: the chunkname beside the message")
  t.eq(body, "x.lua:47: boom", "at: line 47 of the body is line 47 of the message, through the wrapper")

  _, v, body = eval(E, frame, "3-b", "state: gui\nchunkname: =probe\n", boom)
  t.eq(body, "probe:47: boom", "eq: under the = name")
  _, v, body = eval(E, frame, "3-c", "state: gui\nchunkname: probe\n", boom)
  t.eq(body, '[string "probe"]:47: boom', "bare: under a bare name")
  _, v, body = eval(E, frame, "3-d", "state: gui\n", boom)
  t.eq(v.chunkname, "=dcs-eval", "none: the default is echoed")
  t.eq(body, "dcs-eval:47: boom", "none: and the message is under it")
  _, v, body = eval(E, frame, "3-e", "state: gui\nchunkname: @x.lua\n", (boom:gsub("\n", "\r\n")))
  t.eq(body, "x.lua:47: boom", "crlf: CRLF endings count the same")

  order, v, body = eval(E, frame, "3-f", "state: gui\nchunkname: @x.lua\n", at47("local = 1"))
  fields(order, ERR, "compile")
  t.eq(v.stage, "compile", "compile: a compile error in the state is stage compile")
  t.eq(v.chunkname, "@x.lua", "compile: with the chunkname")
  t.check(body:find("^x%.lua:47: "), "compile: and Lua's message names line 47: " .. body)

  local long = "@" .. string.rep("d", 70) .. "\\x.lua"
  _, v, body = eval(E, frame, "3-g", "state: gui\nchunkname: " .. long .. "\n", boom)
  t.eq(v.chunkname, long, "long: the header carries the whole name")
  t.check(body:find("^%.%.%."), "long: Lua abbreviates it in the message: " .. body)
  t.check(body:find("x%.lua:47: boom$"), "long: and the line is still 47: " .. body)

  _, v, body = eval(E, frame, "3-h", "state: mission\nchunkname: @m.lua\n", boom)
  t.eq(body, "m.lua:47: boom", "mission: line 47 is true in a sanitised state too")

  _, v, body = eval(E, frame, "3-i", "state: gui\n", "error({})")
  t.eq(body, "(error object is a table value)", "table raise: named by type in the state")
  _, v, body = eval(E, frame, "3-j", "state: gui\n", "error(0.1 + 0.2, 0)")
  t.eq(body, "0.30000000000000004", "number raise: printed under the same rule")
  _, v, body = eval(E, frame, "3-k", "state: gui\n", 'error("flat", 0)')
  t.eq(body, "flat", "level 0: carried as it is")

  t.eq(E.raised, 0, "lines: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The chunk runs in the target state's globals
--------------------------------------------------------------------------------

do
  local E, env, host, frame = loaded("hook")
  local seen = {}
  local gui, mission = t.state("gui", host), t.state("mission", host)
  carrier(env, { gui = gui, mission = mission }, seen)

  local _, v, body = eval(E, frame, "4-a", "state: gui\n", "dostring_seen = 5")
  t.eq(v.status, "ok", "globals: a chunk that sets a global runs")
  t.eq(rawget(gui, "dostring_seen"), 5, "globals: and the global landed in the target state")
  t.eq(rawget(env, "dostring_seen"), nil, "globals: not in the hook's")
  _, v, body = eval(E, frame, "4-b", "state: gui\n", "return dostring_seen")
  t.eq(body, "5", "globals: the next chunk in that state reads it")
  _, v, body = eval(E, frame, "4-c", "state: mission\n", "return dostring_seen")
  t.eq(v.status, "error", "globals: a chunk in another state does not: the model raises on the name")
  t.eq(v.stage, "run", "globals: while running")
  t.check(body:find("mission%.dostring_seen is not modelled"), "globals: in that state: " .. body)
  _, v, body = eval(E, frame, "4-d", "state: gui\n", 'return rawget(_G, "' .. NAME .. '")')
  t.eq(v.result_type, "nil", "globals: the hook's namespace is not visible in the target state")
  _, v, body = eval(E, frame, "4-e", "state: gui\n", 'return rawget(_G, "NAME")')
  t.eq(v.result_type, "nil", "globals: nor a local of the executor")
  _, v, body = eval(E, frame, "4-f", "state: gui\n", 'return rawget(_G, "finish")')
  t.eq(v.result_type, "nil", "globals: nor a local of the wrapper")
  _, v, body = eval(E, frame, "4-g", "state: mission\n", "return type(string.format)")
  t.eq(body, "function", "sanitised: the base library serves the conversion in mission")
  _, v, body = eval(E, frame, "4-h", "state: mission\n", "return io")
  t.eq(v.status, "error", "sanitised: and io is not there to read")

  t.eq(E.raised, 0, "globals: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The body crosses as a literal
--------------------------------------------------------------------------------

do
  local E, env, host, frame = loaded("hook")
  local seen = {}
  carrier(env, { gui = t.state("gui", host), server = t.state("scripting", host) }, seen)

  -- The body holds a raw NUL byte before a digit, so the literal the
  -- wrapper renders must pad it, `\000`, or the digit is read as part of
  -- the escape.
  local nul = 'return "a\0' .. '1b"'
  local _, v, body = eval(E, frame, "5-a", "state: gui\nchunkname: @x.lua\n", nul)
  t.eq(body, "a\0001b", "nul: a NUL before a digit in the body decodes byte for byte")
  t.check(seen[#seen].chunk:find('a\\0001b', 1, true), "nul: because the literal pads it as \\000")
  local chunk = seen[#seen].chunk
  local literal = string.format("%q", nul)
  t.check(chunk:find(literal, 1, true), "literal: the wrapper carries the body as its %q literal")
  t.check(chunk:find("loadstring(" .. literal .. ', "@x.lua")', 1, true),
    "literal: compiled in the state under the chunkname, with nothing in front of it")
  t.eq(select(2, chunk:gsub("a\\0001b", "")), 1, "literal: the body's text appears once, as the literal")

  local boom = at47('error("boom")')
  _, v, body = eval(E, frame, "5-b", "state: gui\nchunkname: @x.lua\n", boom)
  chunk = seen[#seen].chunk
  t.check(chunk:find(string.format("%q", boom), 1, true), "lines: 47 lines cross as one literal")
  t.eq(body, "x.lua:47: boom", "lines: and line 47 is still 47")

  _, v, body = eval(E, frame, "5-c", "state: server\n", 'return "here"')
  t.eq(v.status, "ok", "server: served")
  t.eq(body, "here", "server: with the value")
  t.eq(seen[#seen].state, "server", "server: the carrier is asked under the name the request spelt")

  t.eq(E.raised, 0, "literal: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The ceiling is applied in the state
--------------------------------------------------------------------------------

do
  local E, env, host, frame = loaded("hook")
  local seen = {}
  carrier(env, { gui = t.state("gui", host) }, seen)
  local ceiling = tonumber(published(E, "max_result_bytes"))
  t.eq(ceiling, 65536, "ceiling: the handshake publishes the figure")
  ---@cast ceiling integer

  local _
  local order, v, body = eval(E, frame, "6-a", "state: gui\n", "return string.rep('x', " .. ceiling .. ")")
  fields(order, OK, "at")
  t.eq(#body, ceiling, "at: a result of exactly the ceiling crosses whole")

  order, v, body = eval(E, frame, "6-b", "state: gui\n", "return string.rep('x', " .. (ceiling + 1) .. ")")
  fields(order, OVERSIZE, "over")
  t.eq(v.status, "error", "over: one byte over is refused")
  t.eq(v.stage, "oversize", "over: under stage oversize")
  t.eq(v.result_bytes, tostring(ceiling + 1), "over: result_bytes is the length refused")
  t.eq(body, "the result is " .. (ceiling + 1) .. " bytes, over the " .. ceiling
    .. "-byte ceiling, and was refused whole rather than cut", "over: the body is the refusal")
  t.eq(seen[#seen].answered, "oversize\nresult\n" .. (ceiling + 1),
    "over: what crossed back was the refusal, so the result never left the state")

  order, v, body = eval(E, frame, "6-c", "state: gui\n", "error(string.rep('z', " .. (ceiling + 1) .. "), 0)")
  fields(order, OVERSIZE, "raise over")
  t.eq(v.stage, "oversize", "raise over: a raise over the ceiling is refused the same way")
  t.eq(body, "the error message is " .. (ceiling + 1) .. " bytes, over the " .. ceiling
    .. "-byte ceiling, and was refused whole rather than cut", "raise over: the body says it was the message")
  t.check(#seen[#seen].answered < 100, "raise over: and the message never left the state")

  -- An unshaped answer over the ceiling is refused too, because the reply
  -- is what the ceiling bounds.
  answering(env, function()
    return string.rep("w", ceiling + 1)
  end)
  order, v, body = eval(E, frame, "6-d", "state: gui\n", "return 1")
  fields(order, OVERSIZE, "unshaped over")
  t.eq(v.stage, "oversize", "unshaped over: refused")
  t.eq(v.result_bytes, tostring(ceiling + 1), "unshaped over: with its length")
  t.check(body:find("^the answer is "), "unshaped over: named as the state's answer: " .. body)

  t.eq(E.raised, 0, "ceiling: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- Refusals
--------------------------------------------------------------------------------

do
  local E, env, host, frame = loaded("hook")
  local seen = {}
  carrier(env, { gui = t.state("gui", host) }, seen)

  local without = t.state("gui", host)
  without.loadstring = nil
  carrier(env, { gui = without }, seen)
  local order, v, body = eval(E, frame, "7-b", "state: gui\n", "return 1")
  fields(order, HEAD, "no loadstring")
  t.eq(v.status, "unsupported", "no loadstring: a state without one cannot compile")
  t.eq(body, "no loadstring in this state", "no loadstring: saying so, from inside the state")

  without = t.state("gui", host)
  without.setfenv = nil
  carrier(env, { gui = without }, seen)
  order, v, body = eval(E, frame, "7-c", "state: gui\n", "return 1")
  fields(order, HEAD, "no setfenv")
  t.eq(v.status, "unsupported", "no setfenv: a state without one cannot run a chunk in its globals")
  t.eq(body, "no setfenv in this state", "no setfenv: saying so")

  -- A wrapper that raises on its own is the executor's failure, answered
  -- and never escaped: here, a state whose `string` is gone from under
  -- the conversion.
  without = t.state("gui", host)
  without.string = nil
  carrier(env, { gui = without }, seen)
  order, v, body = eval(E, frame, "7-d", "state: gui\n", "return 1")
  fields(order, ERR, "wrapper raise")
  t.eq(v.stage, "bridge", "wrapper raise: the wrapper's own raise is stage bridge")
  t.check(body:find("string", 1, true), "wrapper raise: with the message: " .. body)

  local net = env.net
  env.net = nil
  order, v, body = eval(E, frame, "7-e", "state: gui\n", "return 1")
  fields(order, HEAD, "no net")
  t.eq(v.status, "unsupported", "no net: a host without net cannot carry")
  t.eq(body, "no net.dostring_in on this host", "no net: saying so")
  env.net = net

  t.eq(entries(env, E.req), "", "refusals: every request is taken")
  t.eq(E.raised, 0, "refusals: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The model's own stub, and the export host
--------------------------------------------------------------------------------

do
  local E, _, _, frame = loaded("hook")
  local _
  local order, v, body = eval(E, frame, "8-a", "state: gui\n", "return 1")
  fields(order, ERR, "stub")
  t.eq(v.status, "error", "stub: the model's stub evaluates nothing, and its empty answer is not an ok")
  t.eq(v.stage, "dostring_in", "stub: under stage dostring_in")
  t.eq(body, "", "stub: with the empty answer as the body")
  _, v, body = eval(E, frame, "8-b", "state: export\n", "return 1")
  t.eq(v.stage, "dostring_in", "stub: export at the menu is what the stub models as reachable")
  t.eq(E.raised, 0, "stub: nothing reached the guard")
end

do
  local E, _, _, frame = loaded("export")
  local _
  local order, v, body = eval(E, frame, "9-a", "state: gui\n", "return 1")
  fields(order, HEAD, "export gui")
  t.eq(v.status, "unsupported", "export: gui is not a state this host serves")
  t.eq(body, "gui is not a state this host serves", "export: saying so, because the host has no net")
  _, v, body = eval(E, frame, "9-b", "state: export\n", 'return "own"')
  t.eq(v.status, "ok", "export: its own state is served in place")
  t.eq(body, "own", "export: with the value")
  t.eq(E.raised, 0, "export: nothing reached the guard")
end
