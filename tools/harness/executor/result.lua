-- What an `eval` answers with, and the ceiling on it, driven through the
-- local carrier over a sandbox the way `executor/eval-hook` drives it.
-- That suite proves the carrier; this one proves what comes back.
--
-- What is proved. A number is printed so that a reader gets the same
-- double back: `%.14g` where that reads back, `%.17g` where it does not,
-- so `0.1 + 0.2` reads `0.30000000000000004`, `2^53` reads every digit,
-- and a number that `%.14g` serves well is left short. `inf`, `-inf` and
-- `nan` are named, never printed by the C runtime. A boolean is its name,
-- a string its bytes, and nil, a table, a function, a thread and a
-- userdata are an empty body with the type in the header; a `__tostring`
-- the chunk installed is never run, even one that would raise. A result
-- over the handshake's `max_result_bytes` is refused whole under
-- `stage: oversize` with `result_bytes`, and the refusal's body is not one
-- byte of the result; one at the ceiling passes; a raise over it is
-- refused the same way, because the ceiling bounds the reply. The figure
-- the refusal names is the figure the handshake published.
--
-- The mutations this suite exists to catch. Print a number with `%.14g`
-- alone and `0.1 + 0.2` reads `0.3`. Print `nan` with `%g` and the body
-- is the runtime's spelling, `-nan(ind)` here; `inf` happens to print as
-- its name on this runtime, so that check guards the rule and not the
-- host. Stringify a table with `tostring` and the body is not empty, or
-- the flag a `__tostring` sets is set. Cut a result at the ceiling instead
-- of refusing it and the reply is `ok` with nine headers where the
-- refusal has ten. Exempt a raise from the ceiling and the raise-over
-- check reads the same.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The reply's headers, in order. The suite's own copy, kept apart from the
-- executor's on purpose. OK is a value answered, OVERSIZE a value refused.
local OK = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "result_type", "chunkname" }
local ERR = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage", "chunkname" }
local OVERSIZE = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "stage", "chunkname", "result_bytes" }

-- A hook state over a fresh sandbox with the executor loaded into it.
-- Returns the namespace, the state and the frame callback.
local function loaded()
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP }
  local env = t.state("hook", host)
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(type(E), "table", "the executor loaded over hook")
  return E, env, host.callbacks.onSimulationFrame
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

-- An `eval` in `hook` for the session under `id`, answered on one frame
-- and read back.
local function eval(E, frame, id, body)
  request(E, id .. ".req", "op: eval\nfor: " .. E.stamp .. "\nstate: hook\n\n" .. body)
  frame()
  return read(E, id)
end

-- The handshake's value for one header, read with the suite's own reader.
local function published(E, name)
  local fh = assert(io.open(E.handshake, "rb"))
  local bytes = fh:read("*a")
  fh:close()
  return bytes:match("\n" .. name .. ": ([^\n]*)\n")
end

--------------------------------------------------------------------------------
-- Numbers read back
--------------------------------------------------------------------------------

do
  local E, _, frame = loaded()

  -- Each row is an expression and what its body must read. The expression
  -- is also run here, so the body is checked to read back as the double
  -- the chunk returned, and not only to look right.
  local rows = {
    { "42", "42", "an integer" },
    { "-7", "-7", "a negative integer" },
    { "0.5", "0.5", "a fraction %.14g serves" },
    { "3.14", "3.14", "a short decimal, left short" },
    { "0.1", "0.1", "one tenth, left short: %.14g reads back" },
    { "0.1 + 0.2", "0.30000000000000004", "a sum %.14g would round to 0.3, widened" },
    { "1 / 3", "0.33333333333333331", "a third, widened" },
    { "2 ^ 53", "9007199254740992", "2^53, every digit: %.14g would print 9.007199254741e+15" },
    { "2 ^ 53 + 2", "9007199254740994", "2^53 + 2, told apart from 2^53" },
    { "123456789012345678", "1.2345678901234568e+17", "a long integer beyond 2^53, widened in exponent form" },
    { "1e100", "1e+100", "a large power of ten, %.14g's form" },
    { "1e-300", "1e-300", "a small one" },
    { "1e15", "1e+15", "10^15 as %.14g prints it, which reads back" },
    { "100", "100", "a round hundred" },
    { "0", "0", "zero" },
  }
  for i, row in ipairs(rows) do
    local expr, want, what = row[1], row[2], row[3]
    local order, v, body = eval(E, frame, "1-" .. i, "return " .. expr)
    fields(order, OK, what)
    t.eq(v.status, "ok", what .. ": ok")
    t.eq(v.result_type, "number", what .. ": typed")
    t.eq(body, want, what .. ": " .. expr .. " prints as " .. want)
    local value = assert(loadstring("return " .. expr))()
    t.eq(tonumber(body), value, what .. ": and the body reads back as the same double")
  end

  local _, v, body = eval(E, frame, "1-inf", "return 1 / 0")
  t.eq(v.result_type, "number", "inf: typed")
  t.eq(body, "inf", "inf: named")
  _, v, body = eval(E, frame, "1-neg-inf", "return -1 / 0")
  t.eq(body, "-inf", "-inf: named")
  _, v, body = eval(E, frame, "1-nan", "return 0 / 0")
  t.eq(v.result_type, "number", "nan: typed")
  t.eq(body, "nan", "nan: named, never the runtime's " .. tostring(0 / 0))
  _, v, body = eval(E, frame, "1-huge", "return math.huge")
  t.eq(body, "inf", "math.huge: is inf")

  _, v, body = eval(E, frame, "1-raise", "error(0.1 + 0.2, 0)")
  t.eq(v.status, "error", "number raise: error")
  t.eq(body, "0.30000000000000004", "number raise: a number raised with is printed under the same rule")

  t.eq(E.raised, 0, "numbers: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The other types
--------------------------------------------------------------------------------

do
  local E, env, frame = loaded()

  local order, v, body = eval(E, frame, "2-a", "return true")
  local _
  fields(order, OK, "true")
  t.eq(v.result_type, "boolean", "true: typed")
  t.eq(body, "true", "true: named")
  _, v, body = eval(E, frame, "2-b", "return false")
  t.eq(v.result_type, "boolean", "false: typed")
  t.eq(body, "false", "false: named, and not an empty body")

  _, v, body = eval(E, frame, "2-c", 'return "  spaced\\r\\n\\0bytes  "')
  t.eq(v.result_type, "string", "string: typed")
  t.eq(body, "  spaced\r\n\0bytes  ", "string: the bytes verbatim, whitespace, CRLF and NUL kept")
  _, v, body = eval(E, frame, "2-d", 'return ""')
  t.eq(v.status, "ok", "empty string: ok")
  t.eq(v.result_type, "string", "empty string: typed as a string")
  t.eq(body, "", "empty string: an empty body, told from nil by the header")

  _, v, body = eval(E, frame, "2-e", "return nil")
  t.eq(v.result_type, "nil", "nil: typed")
  t.eq(body, "", "nil: an empty body")

  _, v, body = eval(E, frame, "2-f", "return { a = 1 }")
  t.eq(v.result_type, "table", "table: typed")
  t.eq(body, "", "table: an empty body, never serialised")
  _, v, body = eval(E, frame, "2-g", "return type")
  t.eq(v.result_type, "function", "function: typed")
  t.eq(body, "", "function: an empty body")
  _, v, body = eval(E, frame, "2-h", "return coroutine.create(function() end)")
  t.eq(v.result_type, "thread", "thread: typed")
  t.eq(body, "", "thread: an empty body")

  -- The model carries no `newproxy` and no library that hands out a
  -- userdata, so the suite plants one in the state for the chunk to return.
  local tostring_ran = false
  local plain = newproxy(true)
  rawset(env, "result_userdata", plain)
  _, v, body = eval(E, frame, "2-i", "return result_userdata")
  t.eq(v.result_type, "userdata", "userdata: typed")
  t.eq(body, "", "userdata: an empty body")

  _, v, body = eval(E, frame, "2-j",
    "return setmetatable({}, { __tostring = function() result_tostring_ran = true; return 'ran' end })")
  t.eq(v.status, "ok", "__tostring: ok")
  t.eq(v.result_type, "table", "__tostring: typed as a table")
  t.eq(body, "", "__tostring: the body is empty")
  t.eq(rawget(env, "result_tostring_ran"), nil, "__tostring: and the metamethod never ran")

  _, v, body = eval(E, frame, "2-k",
    "return setmetatable({}, { __tostring = function() error('a tostring that raises') end })")
  t.eq(v.status, "ok", "raising __tostring: still ok, because it is not called")
  t.eq(body, "", "raising __tostring: with an empty body")

  local proxy = newproxy(true)
  getmetatable(proxy).__tostring = function()
    tostring_ran = true
    return "ran"
  end
  rawset(env, "result_userdata", proxy)
  _, v, body = eval(E, frame, "2-l", "return result_userdata")
  t.eq(v.result_type, "userdata", "userdata __tostring: typed")
  t.eq(body, "", "userdata __tostring: an empty body")
  t.eq(tostring_ran, false, "userdata __tostring: and never run")

  t.eq(E.raised, 0, "types: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- The ceiling
--------------------------------------------------------------------------------

do
  local E, env, frame = loaded()
  local ceiling = tonumber(published(E, "max_result_bytes"))
  t.check(ceiling and ceiling > 0, "ceiling: the handshake publishes max_result_bytes as a number")
  t.eq(ceiling, 65536, "ceiling: and it is the specification's 65,536")
  ---@cast ceiling integer

  local order, v, body = eval(E, frame, "3-a", "return string.rep('x', " .. ceiling .. ")")
  fields(order, OK, "at")
  t.eq(v.status, "ok", "at: a result of exactly the ceiling passes")
  t.eq(#body, ceiling, "at: whole")

  order, v, body = eval(E, frame, "3-b", "return string.rep('x', " .. (ceiling + 1) .. ")")
  fields(order, OVERSIZE, "over")
  t.eq(v.status, "error", "over: one byte over is refused")
  t.eq(v.stage, "oversize", "over: under stage oversize")
  t.eq(v.chunkname, "=dcs-eval", "over: the chunkname is beside it, because a chunk was compiled")
  t.eq(v.result_bytes, tostring(ceiling + 1), "over: result_bytes is the length that was refused")
  t.eq(body, "the result is " .. (ceiling + 1) .. " bytes, over the " .. ceiling
    .. "-byte ceiling, and was refused whole rather than cut", "over: the body is the refusal")
  t.check(not body:find("^x"), "over: and not one byte of the result")
  t.check(#body < ceiling, "over: the refusal is small")

  local big = ceiling * 10
  order, v, body = eval(E, frame, "3-c", "return string.rep('y', " .. big .. ")")
  fields(order, OVERSIZE, "big")
  t.eq(v.status, "error", "big: ten times the ceiling is refused")
  t.eq(v.result_bytes, tostring(big), "big: naming the whole length")
  t.check(body ~= string.rep("y", ceiling), "big: never cut to the ceiling")
  t.check(body ~= string.rep("y", #body), "big: never a prefix of the result")

  order, v, body = eval(E, frame, "3-d", "error(string.rep('z', " .. (ceiling + 1) .. "), 0)")
  fields(order, OVERSIZE, "raise over")
  t.eq(v.status, "error", "raise over: a raise over the ceiling is error")
  t.eq(v.stage, "oversize", "raise over: under oversize, not run")
  t.eq(v.result_bytes, tostring(ceiling + 1), "raise over: with the message's length")
  t.eq(body, "the error message is " .. (ceiling + 1) .. " bytes, over the " .. ceiling
    .. "-byte ceiling, and was refused whole rather than cut", "raise over: the body says it was the message")

  order, v, body = eval(E, frame, "3-e", "error(string.rep('z', " .. ceiling .. "), 0)")
  fields(order, ERR, "raise at")
  t.eq(v.stage, "run", "raise at: a raise of exactly the ceiling is carried as a run error")
  t.eq(#body, ceiling, "raise at: whole")

  order, v, body = eval(E, frame, "3-f", "return 1")
  fields(order, OK, "after")
  t.eq(body, "1", "after: the next request is answered as before")

  local names = {}
  for name in env.lfs.dir(E.req) do
    if name ~= "." and name ~= ".." then
      names[#names + 1] = name
    end
  end
  t.eq(#names, 0, "ceiling: every request is taken")
  t.eq(E.raised, 0, "ceiling: nothing reached the guard")
  t.eq(E.unpublished, 0, "ceiling: every reply was published")
end
