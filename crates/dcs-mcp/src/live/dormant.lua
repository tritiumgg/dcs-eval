-- The dormant frame's own operations, timed inside DCS by the executor that
-- makes them. `dcs-mcp live dormant` sends this chunk to the host's local
-- state; ADR 0025 holds why the cost is taken this way and not off DCS's
-- frame counter.
--
-- Five operations, each repeated until the clock has moved SPAN seconds or
-- CAP calls have run, whichever is first, and reported in microseconds per
-- call:
--
--   absent_us   the stat a dormant frame makes, on a path that is not there
--   present_us  the same stat on the arm file, which is there while this runs
--   list_us     a listing of the request directory, the armed-idle frame's
--   pcall_us    pcall of a function bumping a table field: the wrapper floor
--   empty_us    an empty call, which the client subtracts from the four above
--
-- and a last line naming every loop the cap ended before the clock did.
--
-- The clock is read once per EVERY calls, so its own cost is spread thin.
-- Every name is taken with `rawget`, the way the executor takes its own, so
-- the chunk runs in a state whose globals raise on an unknown name.
local E = rawget(_G, "DcsEvalExecutor")
local lfs = rawget(_G, "lfs")
local clock = rawget(rawget(_G, "os"), "clock")
local pcall = rawget(_G, "pcall")
local format = rawget(rawget(_G, "string"), "format")
local concat = rawget(rawget(_G, "table"), "concat")

local SPAN, CAP, EVERY = 0.05, 1000000, 64

-- The executor is armed while this chunk runs, so the arm file is present.
-- A dormant frame stats it while it is absent, and its sibling here is the
-- like-for-like path: the same directory, a name that is not there.
local absent = E.arm .. ".absent"

local attributes = lfs.attributes

-- Microseconds per call of `body`, and whether the cap ended the loop.
local function timed(body)
  local n = 0
  local began = clock()
  local now = began
  repeat
    for _ = 1, EVERY do
      body()
    end
    n = n + EVERY
    now = clock()
  until now - began >= SPAN or n >= CAP
  return (now - began) * 1000000 / n, now - began < SPAN
end

local counter = { n = 0 }
local function bump()
  counter.n = counter.n + 1
end

local ops = {
  { "absent_us", function() attributes(absent, "mode") end },
  { "present_us", function() attributes(E.arm, "mode") end },
  { "list_us", function() for _ in lfs.dir(E.req) do end end },
  { "pcall_us", function() pcall(bump) end },
  { "empty_us", function() end },
}

local lines, capped = {}, {}
for i = 1, #ops do
  local op = ops[i]
  local us, cap = timed(op[2])
  lines[#lines + 1] = format("%s %.6f", op[1], us)
  if cap then
    capped[#capped + 1] = op[1]
  end
end
lines[#lines + 1] = "capped " .. (#capped > 0 and concat(capped, ",") or "none")
return concat(lines, "\n")
