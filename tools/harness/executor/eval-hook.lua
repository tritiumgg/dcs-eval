-- The `eval` op through the local carrier: the body compiled with
-- `loadstring` under the request's `chunkname`, run with `setfenv` into
-- the host's own `_G`, and answered on the frame, driven over a sandbox
-- the way `executor/ping` drives the tick. `loadstring` here is the
-- model's own, the reference interpreter's, so the chunks in this suite
-- really run.
--
-- What is proved. A chunk's first return value comes back typed: a string
-- verbatim, a number and a boolean printed, and nil, a table and a
-- function as an empty body with the type in the header. The chunk runs
-- in the host's globals: a global one chunk sets the next reads, the
-- namespace is readable, and no local of the executor is. Line numbers
-- are true, which is the point of the carrier: 46 lines then a raise on
-- line 47 is reported `<name>:47:` under every spelling of `chunkname`,
-- `@x.lua` as `x.lua`, `=probe` as `probe`, a bare name as
-- `[string "probe"]`, and none as `dcs-eval`; a compile error on line 47
-- names 47 under `stage: compile`; CRLF endings count the same; a long
-- name is abbreviated in the message and whole in the header. What a
-- chunk raises with is the body: a string verbatim, a number printed, any
-- other value named by type. A request that names no state, a state that
-- is not a name, or a chunkname over the limit is `bad-request`; a state
-- no host serves, one declared with a carrier not yet built, `server` as
-- another name for `scripting`, and a state with no `loadstring` are
-- `unsupported`; every refusal carries the seven headers and no
-- `chunkname`. A copy with `ALLOW_EVAL` off publishes `eval: disabled`
-- and `ops: ping` and answers every `eval` `unsupported`, an empty one
-- included. The export host serves `export` the same way and refuses
-- `hook`.
--
-- The mutations this suite exists to catch. Prepend one line to the body
-- before compiling and every `:47:` reads `:48:`. Default a missing
-- `chunkname` to the specification's inherited name and the bare case
-- reads `dcs-api-eval`. Drop `setfenv` and a global set by one chunk is
-- gone for the next. Stringify a table result and the body is not empty.
-- Put `chunkname` on a refusal and the field count reads one long.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The reply's headers, in order. The suite's own copy, kept apart from the
-- executor's on purpose. HEAD is a refusal; OK and ERR are an `eval` that
-- was compiled.
local HEAD = { "status", "protocol", "host", "stamp", "phase", "id", "tick" }
local OK = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "result_type", "chunkname" }
local ERR = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage", "chunkname" }

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
local function loaded(state, source)
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP }
  local env = t.state(state, host)
  if source then
    local chunk = assert(loadstring(source, "=" .. NAME))
    setfenv(chunk, env)
    chunk()
  else
    t.load_executor(env)()
  end
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

--------------------------------------------------------------------------------
-- What comes back, by type
--------------------------------------------------------------------------------

do
  local E, env, _, frame = loaded("hook")
  t.eq(type(E.ops.eval), "function", "eval is in the op table")

  local order, v, body = eval(E, frame, "1-a", "state: hook\n", 'return "hi"')
  fields(order, OK, "string")
  t.eq(v.status, "ok", "string: ok")
  t.eq(v.result_type, "string", "string: typed")
  t.eq(v.chunkname, "=dcs-eval", "string: the request named no chunkname, so the default is echoed")
  t.eq(v.tick, "1", "string: answered on the first frame")
  t.eq(body, "hi", "string: the bytes verbatim")
  t.eq(entries(env, E.req), "", "string: the request is gone")

  order, v, body = eval(E, frame, "1-b", "state: hook\n", "return 42")
  fields(order, OK, "number")
  t.eq(v.result_type, "number", "number: typed")
  t.eq(body, "42", "number: printed")

  _, v, body = eval(E, frame, "1-c", "state: hook\n", "return 0.1 + 0.2")
  t.eq(v.result_type, "number", "float: typed")
  t.eq(body, "0.30000000000000004", "float: printed so it reads back, which executor/result proves in full")

  _, v, body = eval(E, frame, "1-d", "state: hook\n", "return true")
  t.eq(v.result_type, "boolean", "boolean: typed")
  t.eq(body, "true", "boolean: named")

  _, v, body = eval(E, frame, "1-e", "state: hook\n", "return nil")
  t.eq(v.status, "ok", "nil: ok")
  t.eq(v.result_type, "nil", "nil: typed")
  t.eq(body, "", "nil: an empty body")

  _, v, body = eval(E, frame, "1-f", "state: hook\n", "local x = 1")
  t.eq(v.result_type, "nil", "no return: nil")
  t.eq(body, "", "no return: an empty body")

  _, v, body = eval(E, frame, "1-g", "state: hook\n", "return { 1, 2 }")
  t.eq(v.status, "ok", "table: ok")
  t.eq(v.result_type, "table", "table: typed")
  t.eq(body, "", "table: never serialised, the body is empty")

  _, v, body = eval(E, frame, "1-h", "state: hook\n", "return type")
  t.eq(v.result_type, "function", "function: typed")
  t.eq(body, "", "function: an empty body")

  _, v, body = eval(E, frame, "1-i", "state: hook\n", "return 1, 2")
  t.eq(v.result_type, "number", "two values: the first")
  t.eq(body, "1", "two values: and only the first")

  _, v, body = eval(E, frame, "1-j", "state: hook\n", 'return "a\\0b\\nc"')
  t.eq(body, "a\0b\nc", "bytes: a NUL and a newline in a string cross verbatim")

  _, v, body = eval(E, frame, "1-k", "state: hook\nchunkname: @x.lua\n", "return 1")
  t.eq(v.chunkname, "@x.lua", "named: the chunkname is echoed as sent")

  t.eq(E.raised, 0, "types: nothing reached the guard")
  t.eq(E.unpublished, 0, "types: every reply was published")
end

--------------------------------------------------------------------------------
-- The chunk runs in the host's globals
--------------------------------------------------------------------------------

do
  local E, env, _, frame = loaded("hook")
  local _, v, body = eval(E, frame, "2-a", "state: hook\n", "eval_hook_seen = 5")
  t.eq(v.status, "ok", "globals: a chunk that sets a global runs")
  t.eq(rawget(env, "eval_hook_seen"), 5, "globals: and the global landed in the host's _G")
  _, v, body = eval(E, frame, "2-b", "state: hook\n", "return eval_hook_seen")
  t.eq(body, "5", "globals: the next chunk reads it")
  _, v, body = eval(E, frame, "2-c", "state: hook\n", 'return rawget(_G, "NAME")')
  t.eq(v.result_type, "nil", "globals: a local of the executor is not visible")
  _, v, body = eval(E, frame, "2-d", "state: hook\n", "return " .. NAME .. ".tick")
  t.eq(body, "4", "globals: the namespace is, and the tick it reads is this frame's")
  _, v, body = eval(E, frame, "2-e", "state: hook\n", "return eval_hook_never")
  t.eq(v.status, "error", "globals: a name the model does not carry raises in the chunk")
  t.eq(v.stage, "run", "globals: while running")
  t.check(body:find("is not modelled", 1, true), "globals: with the model's own message: " .. body)
  t.eq(E.raised, 0, "globals: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- Line numbers are true
--------------------------------------------------------------------------------

do
  local E, _, _, frame = loaded("hook")
  local boom = at47('error("boom")')

  local order, v, body = eval(E, frame, "3-a", "state: hook\nchunkname: @x.lua\n", boom)
  fields(order, ERR, "at")
  t.eq(v.status, "error", "at: a raise is error")
  t.eq(v.stage, "run", "at: while running")
  t.eq(v.chunkname, "@x.lua", "at: the chunkname beside the message")
  t.eq(body, "x.lua:47: boom", "at: line 47 of the body is line 47 of the message, under the @ name")

  _, v, body = eval(E, frame, "3-b", "state: hook\nchunkname: =probe\n", boom)
  t.eq(body, "probe:47: boom", "eq: under the = name")

  _, v, body = eval(E, frame, "3-c", "state: hook\nchunkname: probe\n", boom)
  t.eq(body, '[string "probe"]:47: boom', "bare: under a bare name")

  _, v, body = eval(E, frame, "3-d", "state: hook\n", boom)
  t.eq(v.chunkname, "=dcs-eval", "none: the default is echoed")
  t.eq(body, "dcs-eval:47: boom", "none: and the message is under it")

  _, v, body = eval(E, frame, "3-e", "state: hook\nchunkname: \n", boom)
  t.eq(v.chunkname, "=dcs-eval", "empty: an empty chunkname is none")

  _, v, body = eval(E, frame, "3-f", "state: hook\nchunkname: @x.lua\n", (boom:gsub("\n", "\r\n")))
  t.eq(body, "x.lua:47: boom", "crlf: CRLF endings count the same")

  order, v, body = eval(E, frame, "3-g", "state: hook\nchunkname: @x.lua\n", at47("local = 1"))
  fields(order, ERR, "compile")
  t.eq(v.status, "error", "compile: a compile error is error")
  t.eq(v.stage, "compile", "compile: under stage compile")
  t.eq(v.chunkname, "@x.lua", "compile: with the chunkname")
  t.check(body:find("^x%.lua:47: "), "compile: and Lua's message names line 47: " .. body)

  local long = "@" .. string.rep("d", 70) .. "\\x.lua"
  _, v, body = eval(E, frame, "3-h", "state: hook\nchunkname: " .. long .. "\n", boom)
  t.eq(v.chunkname, long, "long: the header carries the whole name")
  t.check(body:find("^%.%.%."), "long: Lua abbreviates it in the message: " .. body)
  t.check(body:find("x%.lua:47: boom$"), "long: and the line is still 47: " .. body)

  _, v, body = eval(E, frame, "3-i", "state: hook\nchunkname: @x.lua\n", 'error("flat", 0)')
  t.eq(body, "flat", "level 0: a raise without a position is carried as it is")

  t.eq(E.raised, 0, "lines: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- What a chunk raises with
--------------------------------------------------------------------------------

do
  local E, _, _, frame = loaded("hook")
  local order, v, body = eval(E, frame, "4-a", "state: hook\n", "error({})")
  fields(order, ERR, "table raise")
  t.eq(v.stage, "run", "table raise: while running")
  t.eq(body, "(error object is a table value)", "table raise: named by type, never stringified")
  _, v, body = eval(E, frame, "4-b", "state: hook\n", "error(42, 0)")
  t.eq(body, "42", "number raise: printed; at level 0, because error() puts a position on a number as on a string")
  _, v, body = eval(E, frame, "4-c", "state: hook\n", "error(setmetatable({}, { __tostring = function() return 'ran' end }))")
  t.eq(body, "(error object is a table value)", "tostring raise: a __tostring the chunk installed is not run")
  _, v, body = eval(E, frame, "4-d", "state: hook\n", "return setmetatable({}, { __tostring = function() return 'ran' end })")
  t.eq(v.result_type, "table", "tostring result: typed")
  t.eq(body, "", "tostring result: and not run either")
  t.eq(E.raised, 0, "raises: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- Refusals
--------------------------------------------------------------------------------

do
  local E, env, _, frame = loaded("hook")
  local order, v, body = eval(E, frame, "5-a", "", "return 1")
  fields(order, HEAD, "no state")
  t.eq(v.status, "bad-request", "no state: refused")
  t.eq(body, "no state: the request does not name the state to run in", "no state: saying so")

  order, v, body = eval(E, frame, "5-b", "state: \n", "return 1")
  t.eq(v.status, "bad-request", "empty state: refused as none")
  t.eq(body, "no state: the request does not name the state to run in", "empty state: with the same words")

  order, v, body = eval(E, frame, "5-c", "state: 9x\n", "return 1")
  fields(order, HEAD, "shape")
  t.eq(v.status, "bad-request", "shape: a state that is not a name is refused")
  t.eq(body, "state: 9x is not [A-Za-z][A-Za-z0-9_]*", "shape: naming the shape")

  order, v, body = eval(E, frame, "5-d", "state: nope\n", "return 1")
  fields(order, HEAD, "unknown")
  t.eq(v.status, "unsupported", "unknown: a state this host does not declare is unsupported")
  t.eq(body, "nope is not a state this host serves", "unknown: naming it")

  order, v, body = eval(E, frame, "5-e", "state: gui\n", "return 1")
  fields(order, HEAD, "gui")
  t.eq(v.status, "unsupported", "gui: a carrier not yet built is unsupported")
  t.eq(body, "gui is declared and not yet served by this executor", "gui: saying so")

  _, v, body = eval(E, frame, "5-f", "state: server\n", "return 1")
  t.eq(v.status, "unsupported", "server: another name for scripting")
  t.eq(body, "server is declared and not yet served by this executor", "server: refused as scripting is, under its own name")

  _, v, body = eval(E, frame, "5-g", "state: missionscripting\n", "return 1")
  t.eq(body, "missionscripting is declared and not yet served by this executor", "door: the door is not built")

  local long = "@" .. string.rep("y", 200)
  order, v, body = eval(E, frame, "5-h", "state: hook\nchunkname: " .. long .. "\n", "return 1")
  fields(order, HEAD, "long chunkname")
  t.eq(v.status, "bad-request", "long chunkname: over the limit is refused")
  t.eq(body, "chunkname: 201 bytes, over the 200-byte limit", "long chunkname: naming the limit")

  _, v, body = eval(E, frame, "5-i", "state: hook\nchunkname: @" .. string.rep("y", 199) .. "\n", "return 1")
  t.eq(v.status, "ok", "at the limit: 200 bytes is allowed")

  _, v, body = eval(E, frame, "5-j", "state: hook\n", "")
  t.eq(v.status, "bad-request", "empty: a body of nothing is refused before the op")
  t.eq(body, "the body is empty, and eval runs it", "empty: by admit")

  local loadstring = env.loadstring
  env.loadstring = nil
  order, v, body = eval(E, frame, "5-k", "state: hook\n", "return 1")
  fields(order, HEAD, "no loadstring")
  t.eq(v.status, "unsupported", "no loadstring: a state without one cannot compile")
  t.eq(body, "no loadstring in this state", "no loadstring: saying so")
  env.loadstring = loadstring

  local setfenv = env.setfenv
  env.setfenv = nil
  order, v, body = eval(E, frame, "5-m", "state: hook\n", "return 1")
  fields(order, HEAD, "no setfenv")
  t.eq(v.status, "unsupported", "no setfenv: a state without one cannot run a chunk in its globals")
  t.eq(body, "no setfenv in this state", "no setfenv: saying so")
  env.setfenv = setfenv

  _, v, body = eval(E, frame, "5-l", "state: hook\n", "return 1")
  t.eq(v.status, "ok", "after: with both back the state serves again")

  t.eq(entries(env, E.req), "", "refusals: every request is taken")
  t.eq(E.raised, 0, "refusals: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- eval disabled
--------------------------------------------------------------------------------

do
  local fh = assert(io.open(t.root .. "/executor/DcsEvalExecutor.lua", "rb"))
  local source = fh:read("*a")
  fh:close()
  local flipped, n = source:gsub("\nlocal ALLOW_EVAL = true\n", "\nlocal ALLOW_EVAL = false\n")
  t.eq(n, 1, "disabled: the constant is set once, and once is what was flipped")
  local E, _, _, frame = loaded("hook", flipped)
  fh = assert(io.open(E.handshake, "rb"))
  local handshake = fh:read("*a")
  fh:close()
  t.check(handshake:find("\neval: disabled\n", 1, true), "disabled: the handshake says eval is disabled")
  t.check(handshake:find("\nops: ping\n", 1, true), "disabled: and lists ping alone")
  local order, v, body = eval(E, frame, "6-a", "state: hook\n", "return 1")
  fields(order, HEAD, "disabled")
  t.eq(v.status, "unsupported", "disabled: an eval is unsupported")
  t.eq(body, "eval is disabled in this install", "disabled: saying so")
  _, v, body = eval(E, frame, "6-b", "state: hook\n", "")
  t.eq(v.status, "unsupported", "disabled: an empty one is unsupported too, not bad-request")
  t.eq(body, "eval is disabled in this install", "disabled: with the same words")
  _, v, body = eval(E, frame, "6-c", "", "return 1")
  t.eq(v.status, "unsupported", "disabled: and one naming no state, because nothing past the switch is read")
  _, v, body = eval(E, frame, "6-d", "state: nope\n", "return 1")
  t.eq(v.status, "unsupported", "disabled: nor an unknown state")
  t.eq(body, "eval is disabled in this install", "disabled: every eval is one refusal")
end

--------------------------------------------------------------------------------
-- The export host
--------------------------------------------------------------------------------

do
  local E, _, _, frame = loaded("export")
  local order, v, body = eval(E, frame, "7-a", "state: export\nchunkname: @x.lua\n", 'return "here"')
  fields(order, OK, "export")
  t.eq(v.status, "ok", "export: the host's own state is served in place")
  t.eq(v.host, "export", "export: by the export host")
  t.eq(v.result_type, "string", "export: typed")
  t.eq(body, "here", "export: with the value")
  _, v, body = eval(E, frame, "7-b", "state: export\nchunkname: @x.lua\n", at47('error("boom")'))
  t.eq(body, "x.lua:47: boom", "export: line 47 is true here too")
  order, v, body = eval(E, frame, "7-c", "state: hook\n", "return 1")
  fields(order, HEAD, "export hook")
  t.eq(v.status, "unsupported", "export: hook is not a state this host serves")
  t.eq(body, "hook is not a state this host serves", "export: saying so")
  t.eq(E.raised, 0, "export: nothing reached the guard")
end
