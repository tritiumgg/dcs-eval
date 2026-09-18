-- The instruction budget inside a chunk: the count hook set in the state a
-- chunk runs in, driven over a sandbox through all three carriers, the
-- host's own state, `net.dostring_in` with the suite's carrier running the
-- wrapper in a modelled state, and `a_do_script` with the suite's two hops,
-- as `executor/dostring` and `executor/a_do_script` drive them.
--
-- What is proved. The trap the load refuses is Lua's own, pinned first: a
-- count of `0` installs no hook while `debug.gethook` answers the function.
-- A figure that is not a positive integer count, or a default over the
-- ceiling, stops the load in both hosts, named in `dcs.log`. A chunk that
-- loops is stopped `stage: budget`, with the position it was stopped at,
-- in the hook host's own state, the export host's, `gui` and
-- `missionscripting`, and a loop inside a function the chunk calls is
-- stopped too. `mission` has no `debug`, so a chunk there runs unbounded
-- and the reply says `budget: none`. Every reply to a compiled chunk
-- carries `budget` after `chunkname`, a compile error and an oversize
-- result included. `max_instructions` absent or empty is the default, `0`
-- is `none` and unbounded, a count over the ceiling is held to it, and
-- anything but digits is `bad-request`. A chunk that catches the raise
-- with `pcall` gets no further, because a spent budget stays spent. A
-- count that runs out on the executor's own instructions after the chunk
-- returned is not the chunk's: swept from one instruction up, a short
-- chunk reads `budget` and then `ok`, and nothing else. No hook is left
-- set after any chunk, and a hook something else installed is neither
-- displaced nor cleared. A coroutine escapes the budget, pinned as the
-- stated limit it is.
--
-- The mutations this suite exists to catch. Default a zero budget
-- silently instead of refusing it and the load goes on. Drop the check
-- that a firing is `run`'s own and a raise escapes `run` before its clear.
-- Compare the firing against the chunk rather than `run` and the loop in
-- a helper finishes. Drop the re-arm at a count of one and the chunk that
-- catches the raise counts to a thousand. Drop the clear after the chunk
-- and a hook is left set. Install over a hook already set and the other
-- tool's hook is gone. Drop the budget field from the wrapper and the
-- header lists name it missing.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The figures the executor publishes, the suite's own copy.
local BUDGET = 1000000
local CEILING = 50000000

-- The reply's headers, in order. The suite's own copy, kept apart from the
-- executor's on purpose.
local HEAD = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "cpu_ms" }
local OK = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "cpu_ms", "result_type", "chunkname", "budget" }
local ERR = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "cpu_ms", "stage", "chunkname", "budget" }
local OVERSIZE = {
  "status", "protocol", "host", "stamp", "phase", "id", "tick", "cpu_ms", "stage", "chunkname", "budget", "result_bytes",
}

-- What a stopped chunk's message says after its position.
local function stopped(n)
  return "the chunk ran past its budget of " .. n .. " instructions and was stopped"
end

-- The executor's source, read once, for the cases that load a copy with a
-- figure rewritten.
local SOURCE
do
  local fh = assert(io.open(t.root .. "/executor/DcsEvalExecutor.lua", "rb"))
  SOURCE = fh:read("*a")
  fh:close()
end

-- A state over a fresh sandbox with the executor loaded into it, from
-- `source` where one is given. Returns the namespace, the state, the host,
-- the frame callback and the sandbox; the namespace is nil where the load
-- refused.
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
  -- This suite drives the armed path, never the wake: a load is asleep, so
  -- the arm the arm file would do is done here by hand. A load that
  -- published no namespace is a copy this suite expects to have stopped.
  if E then
    E.armed = true
  end
  local frame
  if E and state == "hook" then
    frame = host.callbacks.onSimulationFrame
  elseif E then
    frame = rawget(env, "LuaExportAfterNextFrame")
  end
  return E, env, host, frame, box
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
-- `op` and `for`, answered on one frame and read back. Whatever the chunk
-- did, no hook is left set once the frame is over.
local function eval(E, frame, id, extra, body)
  request(E, id .. ".req", "op: eval\nfor: " .. E.stamp .. "\n" .. (extra or "") .. "\n" .. body)
  frame()
  t.eq(debug.gethook(), nil, id .. ": no hook is left set after the chunk")
  return read(E, id)
end

-- The suite's `net.dostring_in` in the hook state: what it was handed run
-- in the model `targets` names, and "Invalid state name" for any other.
local function carrier(env, targets)
  env.net.dostring_in = function(state, chunk)
    local target = targets[state]
    if not target then
      return "Invalid state name"
    end
    local fn = assert(loadstring(chunk, "=wrapper"))
    setfenv(fn, target)
    return fn()
  end
end

local function pack(...)
  return { n = select("#", ...), ... }
end

-- The suite's `a_do_script`: the far chunk compiled in `far`, run with the
-- arguments it was passed, and answered shifted by one, as DCS was
-- measured to.
local function shifting(far)
  return function(source, ...)
    local fn = assert(loadstring(source, "=far"))
    setfenv(fn, far)
    local got = pack(fn(...))
    local out = { n = got.n }
    for i = 1, got.n - 1 do
      out[i + 1] = got[i]
    end
    return unpack(out, 1, out.n)
  end
end

-- A hook host with a carrier into `gui`, `mission` and, through a loaded
-- mission's `a_do_script`, `missionscripting`. Returns the namespace, the
-- hook state, the frame and the target models by name.
local function opened()
  local E, env, host, frame = loaded("hook")
  t.eq(type(E), "table", "the executor loaded over hook")
  local targets = {
    gui = t.state("gui", host),
    mission = t.state("mission", { mission_loaded = true }),
  }
  targets.missionscripting = t.state("missionscripting", host)
  targets.mission.a_do_script = shifting(targets.missionscripting)
  carrier(env, targets)
  return E, env, frame, targets
end

--------------------------------------------------------------------------------
-- The trap, in Lua itself
--------------------------------------------------------------------------------

do
  local fired = 0
  local function hook()
    fired = fired + 1
  end
  debug.sethook(hook, "", 0)
  local answered = debug.gethook()
  local n = 0
  for i = 1, 100000 do
    n = i
  end
  debug.sethook()
  t.eq(answered, hook, "trap: debug.gethook answers the function a count of 0 was handed")
  t.eq(fired, 0, "trap: and no hook fired over " .. n .. " iterations")
  debug.sethook(hook, "", 1000)
  for i = 1, 100000 do
    n = i
  end
  debug.sethook()
  t.check(fired > 0, "trap: where a positive count fires")
  t.eq(debug.gethook(), nil, "trap: and the clear leaves none")
end

--------------------------------------------------------------------------------
-- A figure Lua would not hook stops the load
--------------------------------------------------------------------------------

do
  local LIMITS = "where Lua hooks a positive integer count up to 2147483647 and installs no hook for 0"
    .. " while debug.gethook says one is set"
  local cases = {
    { "INSTRUCTION_BUDGET", "0", "INSTRUCTION_BUDGET is 0, " .. LIMITS },
    { "INSTRUCTION_BUDGET", "1.5", "INSTRUCTION_BUDGET is 1.5, " .. LIMITS },
    { "INSTRUCTION_BUDGET", "-1", "INSTRUCTION_BUDGET is -1, " .. LIMITS },
    { "INSTRUCTION_BUDGET", '"1000000"', "INSTRUCTION_BUDGET is a string, " .. LIMITS },
    { "INSTRUCTION_BUDGET", "nil", "INSTRUCTION_BUDGET is a nil, " .. LIMITS },
    { "INSTRUCTION_CEILING", "0", "INSTRUCTION_CEILING is 0, " .. LIMITS },
    { "INSTRUCTION_CEILING", "3000000000", "INSTRUCTION_CEILING is 3000000000, " .. LIMITS },
    { "INSTRUCTION_BUDGET", "60000000", "INSTRUCTION_BUDGET is 60000000, over INSTRUCTION_CEILING, 50000000" },
  }
  for _, state in ipairs({ "hook", "export" }) do
    for _, case in ipairs(cases) do
      local what = state .. ", " .. case[1] .. " = " .. case[2]
      local was = case[1] == "INSTRUCTION_BUDGET" and BUDGET or CEILING
      local source, n = SOURCE:gsub("\nlocal " .. case[1] .. " = " .. was .. "\n",
        "\nlocal " .. case[1] .. " = " .. case[2] .. "\n")
      t.eq(n, 1, what .. ": the figure is set once, and once is what was rewritten")
      local E, env, host, _, box = loaded(state, source)
      t.eq(E, nil, what .. ": no namespace is published")
      t.eq(host.callbacks, nil, what .. ": nothing is registered")
      t.eq(env.lfs.attributes(box .. SAVED .. "Logs"), nil, what .. ": nothing is made")
      if state == "hook" then
        t.eq(host.log and #host.log, 1, what .. ": one dcs.log line")
        t.eq(host.log[1].message, "not loaded: " .. case[3], what .. ": naming the figure")
      else
        t.eq(host.log, nil, what .. ": the export state has no log, so the refusal is silent")
      end
    end
  end

  -- The largest count a hook takes, and a default equal to the ceiling, load.
  local source = SOURCE:gsub("\nlocal INSTRUCTION_CEILING = " .. CEILING .. "\n", "\nlocal INSTRUCTION_CEILING = 2147483647\n")
  t.eq(type((loaded("hook", source))), "table", "limits: a ceiling of 2147483647 loads")
  source = SOURCE:gsub("\nlocal INSTRUCTION_BUDGET = " .. BUDGET .. "\n", "\nlocal INSTRUCTION_BUDGET = " .. CEILING .. "\n")
  t.eq(type((loaded("hook", source))), "table", "limits: a default equal to the ceiling loads")
end

--------------------------------------------------------------------------------
-- A looping chunk is stopped where debug exists
--------------------------------------------------------------------------------

do
  local E, _, frame = opened()
  local X, _, _, xframe = loaded("export")
  t.eq(type(X), "table", "the executor loaded over export")

  local where = {
    { "hook", E, frame, {} },
    { "export", X, xframe, {} },
    { "gui", E, frame, {} },
    { "missionscripting", E, frame, { "carrier", "via" } },
  }
  for i, case in ipairs(where) do
    local state, S, step, tail = case[1], case[2], case[3], case[4]
    local function with(list)
      local all = {}
      for _, name in ipairs(list) do
        all[#all + 1] = name
      end
      for _, name in ipairs(tail) do
        all[#all + 1] = name
      end
      return all
    end
    local head = "state: " .. state .. "\nchunkname: =loop\n"

    local order, v, body = eval(S, step, i .. "-a", head .. "max_instructions: 1000\n", "while true do end")
    fields(order, with(ERR), state .. " loop")
    t.eq(v.status, "error", state .. " loop: a chunk that loops is answered")
    t.eq(v.stage, "budget", state .. " loop: under stage budget")
    t.eq(v.budget, "instructions=1000", state .. " loop: saying what bound it")
    t.eq(body, "loop:1: " .. stopped(1000), state .. " loop: where it was stopped")
    if state == "missionscripting" then
      t.eq(v.carrier, "a_do_script", state .. " loop: through a_do_script")
    end

    _, v, body = eval(S, step, i .. "-b", head, "local n = 0\nwhile true do\n  n = n + 1\nend")
    t.eq(v.stage, "budget", state .. " default: stopped with no count named")
    t.eq(v.budget, "instructions=" .. BUDGET, state .. " default: under the default")
    t.check(body:find("^loop:[23]: " .. stopped(BUDGET) .. "$"), state .. " default: inside the loop: " .. body)

    _, v, body = eval(S, step, i .. "-c", head .. "max_instructions: 1000\n",
      "local function spin()\n  for i = 1, 10000000 do end\nend\nspin()\nreturn 'finished'")
    t.eq(v.stage, "budget", state .. " helper: a loop in a function the chunk calls is stopped")
    t.eq(body, "loop:2: " .. stopped(1000), state .. " helper: inside the helper")

    order, v, body = eval(S, step, i .. "-d", head .. "max_instructions: 1000\n", "return 'quick'")
    fields(order, with(OK), state .. " quick")
    t.eq(v.status, "ok", state .. " quick: a chunk inside its budget is answered")
    t.eq(v.budget, "instructions=1000", state .. " quick: and says what bound it")
    t.eq(body, "quick", state .. " quick: with its value")

    order, v, body = eval(S, step, i .. "-e", head .. "max_instructions: 1000\n", "return +")
    fields(order, with(ERR), state .. " compile")
    t.eq(v.stage, "compile", state .. " compile: refused")
    t.eq(v.budget, "instructions=1000", state .. " compile: carrying the budget it would have run under")

    order, v = eval(S, step, i .. "-f", head .. "max_instructions: 1000\n", "return string.rep('x', 65537)")
    fields(order, with(OVERSIZE), state .. " oversize")
    t.eq(v.stage, "oversize", state .. " oversize: refused")
    t.eq(v.budget, "instructions=1000", state .. " oversize: carrying the budget, before result_bytes")

    order, v, body = eval(S, step, i .. "-g", head .. "max_instructions: 1000\n", "error('boom')")
    fields(order, with(ERR), state .. " raise")
    t.eq(v.stage, "run", state .. " raise: a chunk's own raise is not the budget")
    t.eq(body, "loop:1: boom", state .. " raise: with its message")
  end

  t.eq(E.raised, 0, "loops: nothing reached the hook host's guard")
  t.eq(X.raised, 0, "loops: nothing reached the export host's guard")
end

--------------------------------------------------------------------------------
-- mission has no debug
--------------------------------------------------------------------------------

do
  local E, _, frame, targets = opened()
  t.eq(rawget(targets.mission, "debug"), nil, "mission: the model carries no debug, as DCS's mission has none")

  local order, v, body = eval(E, frame, "1-a", "state: mission\n", "return 1")
  fields(order, OK, "mission")
  t.eq(v.status, "ok", "mission: a chunk is answered")
  t.eq(v.budget, "none", "mission: and says nothing bound it")
  t.eq(body, "1", "mission: with its value")

  _, v, body = eval(E, frame, "1-b", "state: mission\nmax_instructions: 10\n",
    "local n = 0\nfor i = 1, 100000 do\n  n = i\nend\nreturn n")
  t.eq(v.status, "ok", "mission unbounded: a count names nothing where no hook can be set")
  t.eq(v.budget, "none", "mission unbounded: none")
  t.eq(body, "100000", "mission unbounded: the loop ran to its end")
end

--------------------------------------------------------------------------------
-- max_instructions
--------------------------------------------------------------------------------

do
  local E, _, frame = opened()
  local LOOP = "local n = 0\nfor i = 1, 100000 do\n  n = i\nend\nreturn n"

  for i, state in ipairs({ "hook", "gui" }) do
    local head = "state: " .. state .. "\n"
    local _, v = eval(E, frame, i .. "-a", head, "return 1")
    t.eq(v.budget, "instructions=" .. BUDGET, state .. " absent: the default")
    _, v = eval(E, frame, i .. "-b", head .. "max_instructions: \n", "return 1")
    t.eq(v.budget, "instructions=" .. BUDGET, state .. " empty: the default, as an empty chunkname is")

    local order, body
    order, v, body = eval(E, frame, i .. "-c", head .. "max_instructions: 0\n", LOOP)
    fields(order, OK, state .. " zero")
    t.eq(v.budget, "none", state .. " zero: nothing bounds the chunk")
    t.eq(body, "100000", state .. " zero: and the loop runs to its end")

    _, v = eval(E, frame, i .. "-d", head .. "max_instructions: 99999999999\n", "return 1")
    t.eq(v.budget, "instructions=" .. CEILING, state .. " over: held to the ceiling")
    _, v = eval(E, frame, i .. "-e", head .. "max_instructions: " .. string.rep("9", 400) .. "\n", "return 1")
    t.eq(v.budget, "instructions=" .. CEILING, state .. " far over: held to the ceiling, not read as infinity")
    _, v = eval(E, frame, i .. "-f", head .. "max_instructions: 000100\n", "return 1")
    t.eq(v.budget, "instructions=100", state .. " leading zeros: digits all the same")

    local refusals = {
      { "abc", "max_instructions: abc is not a non-negative integer" },
      { "-1", "max_instructions: -1 is not a non-negative integer" },
      { "1.5", "max_instructions: 1.5 is not a non-negative integer" },
      { "1e3", "max_instructions: 1e3 is not a non-negative integer" },
      { "0x10", "max_instructions: 0x10 is not a non-negative integer" },
      { "-" .. string.rep("1", 100),
        "max_instructions: -" .. string.rep("1", 79) .. "... is not a non-negative integer" },
    }
    for j, case in ipairs(refusals) do
      order, v, body = eval(E, frame, i .. "-g" .. j, head .. "max_instructions: " .. case[1] .. "\n", "return 1")
      fields(order, HEAD, state .. " " .. case[1])
      t.eq(v.status, "bad-request", state .. " " .. case[1] .. ": refused")
      t.eq(body, case[2], state .. " " .. case[1] .. ": saying so")
    end
  end
end

--------------------------------------------------------------------------------
-- The executor's own instructions are not the chunk's
--------------------------------------------------------------------------------

do
  local E, _, frame = opened()
  for i, state in ipairs({ "hook", "gui" }) do
    local seen, first_ok = {}, nil
    for n = 1, 40 do
      local _, v, body = eval(E, frame, i .. "-" .. n, "state: " .. state .. "\nmax_instructions: " .. n .. "\n",
        "local a = 1\nreturn a")
      seen[n] = v.status .. "/" .. (v.stage or "") .. "/" .. (v.status == "ok" and body or "")
      if v.status == "ok" then
        first_ok = first_ok or n
        t.eq(seen[n], "ok//1", state .. " sweep " .. n .. ": ok with the value")
      else
        t.eq(first_ok, nil, state .. " sweep " .. n .. ": nothing but ok once a count was enough, got " .. seen[n])
        t.eq(seen[n], "error/budget/", state .. " sweep " .. n .. ": a count too small is the budget, never "
          .. seen[n] .. ": " .. body)
      end
    end
    t.check(seen[1] == "error/budget/", state .. " sweep: one instruction is too few")
    t.check(first_ok and first_ok < 40, state .. " sweep: and a few are enough")
  end
  t.eq(E.raised, 0, "sweep: nothing reached the guard")
end

--------------------------------------------------------------------------------
-- A spent budget stays spent
--------------------------------------------------------------------------------

do
  local E, env, frame, targets = opened()
  local CAUGHT = "local function spin()\n  while true do end\nend\n"
    .. "for i = 1, 1000 do\n  pcall(spin)\n  caught = i\nend\nreturn caught"
  for i, case in ipairs({ { "hook", env }, { "gui", targets.gui } }) do
    local state, globals = case[1], case[2]
    local _, v, body = eval(E, frame, i .. "-a", "state: " .. state .. "\nchunkname: =caught\nmax_instructions: 100\n",
      CAUGHT)
    t.eq(v.stage, "budget", state .. " caught: a chunk that catches the raise is still stopped")
    t.eq(rawget(globals, "caught"), nil, state .. " caught: before it takes one step past the catch")
    t.check(body:find("^caught:%d: " .. stopped(100) .. "$"), state .. " caught: inside the chunk: " .. body)
  end
end

--------------------------------------------------------------------------------
-- A hook something else set is neither displaced nor cleared
--------------------------------------------------------------------------------

do
  local E, _, frame = opened()
  local fired = 0
  local function theirs()
    fired = fired + 1
  end
  for i, state in ipairs({ "hook", "gui" }) do
    debug.sethook(theirs, "", 2000000000)
    request(E, i .. "-a.req", "op: eval\nfor: " .. E.stamp .. "\nstate: " .. state .. "\nmax_instructions: 10\n\n"
      .. "local n = 0\nfor i = 1, 10000 do\n  n = i\nend\nreturn n")
    frame()
    local installed = debug.gethook()
    debug.sethook()
    local _, v, body = read(E, i .. "-a")
    t.eq(installed, theirs, state .. " theirs: the other hook is still set after the chunk")
    t.eq(v.status, "ok", state .. " theirs: the chunk ran")
    t.eq(v.budget, "none", state .. " theirs: unbounded, and saying so")
    t.eq(body, "10000", state .. " theirs: to its end")
  end
  t.eq(fired, 0, "theirs: the other hook's count was never reached")
end

--------------------------------------------------------------------------------
-- A coroutine escapes the budget, a limit stated rather than hidden
--------------------------------------------------------------------------------

do
  local E, _, frame = opened()
  for i, state in ipairs({ "hook", "gui" }) do
    local _, v, body = eval(E, frame, i .. "-a", "state: " .. state .. "\nmax_instructions: 1000\n",
      "local n = 0\ncoroutine.wrap(function()\n  for i = 1, 100000 do\n    n = i\n  end\nend)()\nreturn n")
    t.eq(v.status, "ok", state .. " coroutine: a loop inside a coroutine is not counted")
    t.eq(body, "100000", state .. " coroutine: and runs to its end")
    t.eq(v.budget, "instructions=1000", state .. " coroutine: though the reply says what bound the chunk")
  end
end

t.eq(debug.gethook(), nil, "the suite leaves no hook set for the suites after it")
