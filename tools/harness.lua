-- The Lua harness: runs the executor's test suites under the reference
-- interpreter and counts what they checked.
--
--   lua5.1 tools/harness.lua                   every suite
--   lua5.1 tools/harness.lua selftest stubs    the suites named
--   lua5.1 tools/harness.lua executor          every suite under executor/
--
-- Exit 0 when every check passed. Exit 1 when a check failed or a suite
-- raised. Exit 2 when nothing ran: a suite with no checks, a name that
-- matches no suite, or no suites at all. That third code is the point of the
-- runner. A test file with its assertions commented out, or a suite whose
-- name was misspelt on the command line, exits 0 under most runners and looks
-- like a pass. So every gate that runs this asserts the check count it
-- prints, never the exit code alone.
--
-- Each suite prints one line, `<name>: N checks`, and nothing per check.
-- A suite is a chunk under tools/harness/ receiving this runner's API as its
-- one argument (`local t = ...`). A check that fails stops its suite at that
-- line, as assert would; the other suites still run.
--
-- Strict by default, in two places. The harness's own globals are sealed
-- before any suite runs, so a suite that reads or writes a name it never
-- declared raises rather than reading nil. And `t.strict` builds the model
-- tables the suites hand the executor: a name the model does not carry
-- raises where it is read, naming its path, because a stub that answers nil
-- for `DCS.getPuase` turns a typo in the executor into a green run.

-- Where the runner lives, so the suites and the executor are found from any
-- working directory. arg[0] is how the interpreter was told to find this file.
local here = (arg and arg[0] or "tools/harness.lua"):match("^(.*)[/\\][^/\\]*$") or "."
local SUITE_DIR = here .. "/harness/"
local EXECUTOR = here .. "/../executor/DcsEvalExecutor.lua"

-- The suites, in the order they run. A list rather than a directory scan
-- because the reference interpreter cannot list a directory, and because the
-- list is what tools/harness-test.sh replaces to drive this runner against
-- suites that must fail.
local SUITES = dofile(SUITE_DIR .. "suites.lua")

--------------------------------------------------------------------------------
-- The API a suite receives
--------------------------------------------------------------------------------

-- A failed check raises this table rather than a string, so the runner can
-- tell a check that failed from a suite that broke.
local FAIL = {}

local function show(v)
  if type(v) == "string" then
    return string.format("%q", v)
  end
  return tostring(v)
end

local function where(level)
  local info = debug.getinfo(level + 1, "Sl")
  if not info then
    return "?"
  end
  return info.short_src .. ":" .. tostring(info.currentline)
end

local function fail(what, detail, level)
  local msg = where(level + 1) .. ": " .. what
  if detail then
    msg = msg .. "\n    " .. detail
  end
  error({ [FAIL] = msg }, 0)
end

local function api(count)
  local t = {}

  -- Each check counts itself once it has passed, so the count a failed suite
  -- reports is the checks that held before the one that did not.

  -- One check. `what` is the sentence a reader sees when it fails.
  function t.check(cond, what)
    if not cond then
      fail(what or "check failed", nil, 2)
    end
    count.n = count.n + 1
  end

  function t.eq(got, want, what)
    if got ~= want then
      fail(what or "values differ", "wanted " .. show(want) .. ", got " .. show(got), 2)
    end
    count.n = count.n + 1
  end

  -- `fn` must raise, and the message must match `pattern` when one is given.
  function t.raises(fn, pattern, what)
    local ok, err = pcall(fn)
    if ok then
      fail(what or "wanted a raise", "nothing was raised", 2)
    end
    if type(err) == "table" and err[FAIL] then
      -- A failed check inside `fn` is a failure of this suite, not a raise
      -- the suite meant to observe.
      error(err, 0)
    end
    err = tostring(err)
    if pattern and not err:find(pattern) then
      fail(what or "wrong raise", "wanted a message matching " .. show(pattern) .. ", got " .. show(err), 2)
    end
    count.n = count.n + 1
  end

  -- A model of one DCS-provided table: `members` is what it carries, and any
  -- other name raises where it is read, naming its path. Writes are allowed,
  -- because the executor is allowed to define things in the state it runs in
  -- and a model that refused would be modelling a host that does not exist.
  function t.strict(name, members)
    local model = {}
    for k, v in pairs(members or {}) do
      model[k] = v
    end
    return setmetatable(model, {
      __index = function(_, key)
        error("harness: " .. name .. "." .. tostring(key) .. " is not modelled", 2)
      end,
    })
  end

  -- The executor's one file, compiled but not run, with `env` as its globals.
  -- The suite calls the chunk itself, so it can decide what the host looks
  -- like before the first line runs. A missing file is a raise, not nil: a
  -- suite that loaded nothing must not go on to count checks about it.
  function t.load_executor(env)
    local chunk, err = loadfile(EXECUTOR)
    if not chunk then
      error("harness: cannot load the executor: " .. tostring(err), 2)
    end
    if env then
      setfenv(chunk, env)
    end
    return chunk
  end

  return t
end

--------------------------------------------------------------------------------
-- Running suites
--------------------------------------------------------------------------------

local function leaf(name)
  return name:match("([^/]+)$")
end

-- Run one suite. Returns the check count, or nil, a reason and the count
-- reached before the suite stopped.
local function run(name)
  local chunk, err = loadfile(SUITE_DIR .. name .. ".lua")
  if not chunk then
    return nil, "cannot load: " .. tostring(err), 0
  end
  local count = { n = 0 }
  local ok, raised = xpcall(function()
    chunk(api(count))
  end, function(e)
    if type(e) == "table" and e[FAIL] then
      return e
    end
    return debug.traceback(tostring(e), 2)
  end)
  if not ok then
    if type(raised) == "table" then
      return nil, "check failed at " .. raised[FAIL], count.n
    end
    return nil, "raised: " .. tostring(raised), count.n
  end
  return count.n
end

-- Expand what was asked for into suite names. A name is one suite, or a
-- directory of them. Order is the registry's, so a stage's suites run in
-- the order they were built.
local function select(wanted)
  if #wanted == 0 then
    return SUITES, {}
  end
  local chosen, seen, missing = {}, {}, {}
  for _, w in ipairs(wanted) do
    local hit = false
    for _, s in ipairs(SUITES) do
      if s == w or s:sub(1, #w + 1) == w .. "/" then
        hit = true
        if not seen[s] then
          seen[s] = true
          chosen[#chosen + 1] = s
        end
      end
    end
    if not hit then
      missing[#missing + 1] = w
    end
  end
  return chosen, missing
end

local function plural(n)
  return n .. " check" .. (n == 1 and "" or "s")
end

local function main(args)
  local chosen, missing = select(args)
  if #missing > 0 then
    io.stderr:write("harness: no suite named " .. table.concat(missing, ", ") .. "\n")
    io.stderr:write("harness: the suites are " .. table.concat(SUITES, ", ") .. "\n")
    return 2
  end

  local failed, empty, total = 0, 0, 0
  for _, name in ipairs(chosen) do
    local n, reason, reached = run(name)
    if not n then
      failed = failed + 1
      io.stderr:write("FAIL  " .. name .. ": " .. reason .. "\n")
      if reached > 0 then
        io.stderr:write("      after " .. plural(reached) .. " passed\n")
      end
    elseif n == 0 then
      empty = empty + 1
      io.stderr:write("FAIL  " .. name .. ": no checks ran\n")
    else
      total = total + n
      print(leaf(name) .. ": " .. plural(n))
    end
  end

  if #chosen > 1 then
    print("harness: " .. #chosen .. " suites, " .. plural(total))
  end
  if failed > 0 then
    return 1
  end
  if empty > 0 or total == 0 then
    io.stderr:write("harness: nothing ran\n")
    return 2
  end
  return 0
end

-- Seal the globals. From here on a name nobody declared is a raise, in the
-- runner and in every suite. The suites get their tools through `...`, so
-- there is nothing they need to find by name in here.
setmetatable(_G, {
  __index = function(_, key)
    error("harness: undefined global '" .. tostring(key) .. "'", 2)
  end,
  __newindex = function(_, key)
    error("harness: assignment to undeclared global '" .. tostring(key) .. "'", 2)
  end,
})

os.exit(main(arg))
