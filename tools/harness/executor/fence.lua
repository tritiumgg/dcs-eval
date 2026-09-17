-- The stamp fence: a request written for another session, answered
-- `stale-session` and never run, driven over a sandbox by calling the
-- callback the load registered, the way `executor/ping` drives the frame.
--
-- The kill this reproduces. On 2026-09-02 a request written as DCS quit
-- was found by the next launch and its chunk ran there, in a session
-- nobody had addressed. The structural half of the answer is the session
-- directory, which `executor/session` proves: a request left in a dead
-- session's directory is never listed. This is the other half, for the
-- case the structure does not cover — a client that reads one session's
-- handshake and writes into the directory of the one that replaced it —
-- and it is what makes a client defect visible instead of fatal.
--
-- How the chunk is watched. The suite's `loadstring` compiles the
-- executor's own two sources and nothing else: a body that reaches it is
-- counted and comes back as a function that counts itself and then raises,
-- so "the chunk never ran" is a number this suite read and not an absence
-- of evidence. `net.dostring_in` is counted the same way, so a chunk that
-- never compiled here but crossed into another state is caught too: one
-- foreign request names `mission`, which the hook host reaches through
-- that carrier, and another names `missionscripting`, the one state
-- reached two hops out through `a_do_script`, whose first hop is the same
-- call; the same bytes for this session are sent across it, so
-- the send count is one this suite has watched fill — on the hook host,
-- which is where it can move at all: the export state has no `net`, so the
-- send count is the one thing the export block's control cannot prove. Its
-- other two are live, and only because the foreign request there names
-- `export`: the export host serves that state alone, so a request naming
-- another is `unsupported` with the fence taken out and would leave all
-- three counts at zero whatever the fence did. A body reaches the
-- watch under whatever `chunkname` its request named, and a compile whose
-- name begins `=DcsEvalExecutor.` passes through to the real `loadstring`
-- as the executor's own sources do: a later case that sets `chunkname` and
-- spells that prefix would blind the watch without reddening anything.
--
-- What is proved. A `for` that is not this session's stamp is answered
-- `stale-session` on the frame, with the eight headers every reply
-- carries and then `for`, which echoes the stamp the request named and
-- never the session's own; the body names both. The request is taken off
-- the disk like any other, so it is not left for the next listing, and
-- nothing was compiled, run or sent to a state. That holds for a request
-- already in the session's directory when the executor loaded — a client
-- published it for the session this one replaced — and for one published
-- while the session runs; the same bytes sent for this session compile
-- and run, so the fence and not the fixture is what stopped the first.
-- The stamp alone decides: a foreign request with no op, with an unknown
-- one, with an empty `eval` body, naming a state no host serves, or naming
-- one of the two the hook host reaches across a carrier — `mission` at one
-- hop and `missionscripting` at two — is `stale-session` still, because
-- none of those is looked for; an envelope that does not parse is not,
-- because the fence sits past the envelope and reads a header the parser
-- never handed over. A request for this session refused for one of those
-- second reasons carries the eight headers and no `for`, because the echo
-- is the fence's alone, and it is judged in a session where a foreign
-- request has already filled the slot the echo rides in, so an emptiness
-- nothing ever wrote is not what passes it. The
-- comparison is exact — the stamp with a blank after it, one byte more
-- than it, a prefix of it, the pid alone and the time alone are all
-- foreign, and the stamp itself is not. The header's name is the parser's
-- to spell: a request that wrote `For` is judged the same and its echo
-- comes back under the `for` the executor writes. A stamp longer than a message
-- should carry comes back whole in the header and excerpted in the body,
-- and the cut is past eighty bytes and not at it: eighty arrive whole and
-- eighty-one are cut to eighty and three dots. The header's echo is
-- uncapped, which a stamp of three hundred bytes says and a shorter one
-- cannot. An oversize request
-- is refused unread before the fence sees a header, because the size
-- check comes first. A foreign request among this session's own is
-- answered in name order with them and stops nothing. The export host
-- fences on its own callback, its own stamp and its own state.
--
-- The mutations this suite exists to catch. Drop the comparison and the
-- kill control reads a chunk compiled and run, and the foreign request
-- naming `mission` sent across the carrier. Fence on a prefix instead
-- of an equality and the stamp with a blank after it passes. Put the fence
-- after the op is looked for and the foreign request with no op reads
-- `bad-request`. Echo the session's own stamp in `for` instead of the one
-- asked for and the header reads the wrong one. Drop the excerpt and a
-- stamp of a hundred bytes arrives whole in the message. Excerpt at
-- eighty bytes rather than past them and a stamp of exactly eighty loses
-- its last byte to three dots. Cap the echo at the two hundred bytes ADR
-- 0006 records as the alternative it did not take and the three-hundred
-- byte stamp comes back short. Hang the echo on a refusal that is not the
-- fence's and a request for this session with no op grows a ninth header;
-- hoist the slot the echo rides in out of `admit` and leave it set, which
-- is the refactor that reads as tidying, and the same request grows the
-- ninth carrying the stamp the request before it named.
-- Let the parser take a repeated header and
-- the envelope cases read the fence's nine headers where a bad envelope's
-- eight are wanted, which is what holds the parser ahead of the fence and
-- is the case a later reader is likeliest to think redundant. Exempt one
-- state from the comparison and the foreign request naming it crosses the
-- carrier, which is the send count's own mutation: it reads one sent where
-- it wants none, with the reply shape reddening first. Both carriers are
-- named, because a state exempted at `missionscripting` would be missed by
-- a suite that only ever sends one hop.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]
local ROOT = TEMP .. [[dcs-eval\]]

-- The clock and the pid every load here takes, so the stamp is known
-- before the executor mints it and a request can be planted in the
-- directory the load is about to take over.
local CLOCK = 1000
local PID = 7
local STAMP = CLOCK .. "-" .. PID

-- The reply's headers, in order. The suite's own copy, kept apart from the
-- executor's on purpose: the eight every reply carries, and the one a
-- fenced request adds.
local HEAD = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "cpu_ms" }
local FENCED = { "status", "protocol", "host", "stamp", "phase", "id", "tick", "cpu_ms", "for" }

-- The body of the request the kill ran. Nothing here compiles it, so what
-- it says matters only in that it is not empty and would be code.
local KILL = "return the_session_is_gone()"

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

-- A state over a fresh sandbox with the clock frozen, the pid chosen and
-- the watch on the chunk in place, built but not loaded, so a case can put
-- a file in the session directory first. Returns the state, the host, the
-- sandbox and the counts.
local function prepared(state)
  local box = t.sandbox()
  local host = { writedir = box .. SAVED, tempdir = box .. TEMP, pid = PID }
  local env = t.state(state, host)
  env.os.time = function()
    return CLOCK
  end
  local watch = { compiled = 0, ran = 0, sent = 0 }
  local compile = env.loadstring
  env.loadstring = function(source, chunkname)
    -- The executor compiles its own conversion and hook sources under
    -- names it spells itself; those are not a request's body and pass
    -- through to the model's own `loadstring`.
    if type(chunkname) == "string" and chunkname:find("^=" .. NAME .. "%.") then
      return compile(source, chunkname)
    end
    watch.compiled = watch.compiled + 1
    return function()
      watch.ran = watch.ran + 1
      error("the chunk ran and took the session with it", 0)
    end
  end
  if state ~= "export" then
    local dostring_in = env.net.dostring_in
    env.net.dostring_in = function(name, source)
      watch.sent = watch.sent + 1
      return dostring_in(name, source)
    end
  end
  return env, host, box, watch
end

-- `rest` under `base`, one level at a time through the model.
local function mkdirs(env, base, rest)
  local built = base
  for segment in rest:gmatch("[^\\/]+") do
    built = built .. "\\" .. segment
    if env.lfs.attributes(built, "mode") ~= "directory" then
      assert(env.lfs.mkdir(built))
    end
  end
  return built
end

-- One request in the session directory before the load, put there through
-- the model the way a client would: the directory is named by the stamp
-- this load is about to take, so the file is waiting when the executor
-- opens the session.
local function planted(env, box, host, name, content)
  local dir = mkdirs(env, box, ROOT .. host .. "\\" .. STAMP .. "\\req")
  local fh = assert(env.io.open(dir .. "\\" .. name, "wb"))
  fh:write(content)
  fh:close()
end

-- The executor loaded over a prepared state, with its namespace.
local function load(env)
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(type(E), "table", "the executor loaded and published its namespace")
  t.eq(E.stamp, STAMP, "and took the stamp this suite planted for")
  return E
end

-- A prepared state, loaded, with nothing in the session directory.
local function loaded(state)
  local env, host, box, watch = prepared(state)
  return load(env), env, host, watch, box
end

-- One request under `E.req`, written through the runner's own `io`.
local function request(E, name, content)
  local fh = assert(io.open(E.req .. "\\" .. name, "wb"))
  fh:write(content)
  fh:close()
end

-- A request for `stamp`, whose body is the chunk the kill ran.
local function kill(E, name, stamp, extra)
  request(E, name, "op: eval\nfor: " .. stamp .. "\nstate: hook\n" .. (extra or "") .. "\n" .. KILL)
end

-- A reply read back with the suite's own reader, not the executor's
-- parser: the names in the order written, the values by name, and the
-- body.
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

-- The message the fence writes for a request that named `named`, as this
-- suite spells it.
local function refusal(named)
  if #named > 80 then
    named = named:sub(1, 80) .. "..."
  end
  return "for: " .. named .. " is not this session's stamp, " .. STAMP
    .. ": the request was written for another session and was not run"
end

-- One fenced reply: the eight headers then `for`, the stamp the request
-- named echoed whole, and the message as the body.
local function fenced(E, id, named, what)
  local order, v, body = read(E, id)
  fields(order, FENCED, what)
  t.eq(v.status, "stale-session", what .. ": the status is stale-session")
  t.eq(v.id, id, what .. ": under the name the request came in")
  t.eq(v.stamp, STAMP, what .. ": the session that refused it is this one")
  t.eq(v["for"], named, what .. ": and for echoes the stamp the request named")
  t.eq(body, refusal(named), what .. ": the message names both stamps")
end

-- The kill control: nothing the requests carried was compiled, run or
-- sent into a state.
local function nothing_ran(watch, what)
  t.eq(watch.compiled, 0, what .. ": no chunk was compiled")
  t.eq(watch.ran, 0, what .. ": no chunk ran")
  t.eq(watch.sent, 0, what .. ": nothing was sent to a state")
end

--------------------------------------------------------------------------------
-- A request for another session, on the frame
--------------------------------------------------------------------------------

do
  local E, env, host, watch = loaded("hook")
  kill(E, "0000000001-abcd.req", "999-7")
  host.callbacks.onSimulationFrame()
  -- The kill control first, because it is the one a fence that stopped
  -- fencing reddens: the chunk is what must not have happened, and the
  -- reply is only how a client learns it did not.
  nothing_ran(watch, "on a tick")
  t.eq(E.tick, 1, "one frame is one tick")
  t.eq(entries(env, E.req), "", "the request is taken off the disk like any other")
  t.eq(entries(env, E.res), "0000000001-abcd.res", "and the reply is under res, no .tmp")
  fenced(E, "0000000001-abcd", "999-7", "on a tick")
  t.eq(E.raised, 0, "on a tick: nothing raised")
  t.eq(E.unpublished, 0, "on a tick: the reply was published")

  -- And the request is gone for good: a second frame has nothing to answer
  -- and writes nothing, so a chunk refused once cannot be met again.
  host.callbacks.onSimulationFrame()
  t.eq(entries(env, E.res), "0000000001-abcd.res", "on a tick: the second frame answers nothing more")
  nothing_ran(watch, "on a tick, twice")
end

--------------------------------------------------------------------------------
-- A request already there at load
--------------------------------------------------------------------------------

do
  -- The case the fence exists for: a client read one session's handshake,
  -- the process it named went, and a new session took the same stamped
  -- directory. The request is waiting when the executor opens the session.
  local env, host, box, watch = prepared("hook")
  planted(env, box, "hook", "0000000001-abcd.req",
    "op: eval\nfor: 999-7\nstate: hook\n\n" .. KILL)
  local E = load(env)
  t.eq(entries(env, E.req), "0000000001-abcd.req", "at load: the request survived the load")
  t.eq(entries(env, E.res), "", "at load: and nothing was answered during it")
  nothing_ran(watch, "at load, before the first frame")
  host.callbacks.onSimulationFrame()
  t.eq(entries(env, E.req), "", "at load: the first frame took it")
  fenced(E, "0000000001-abcd", "999-7", "at load")
  nothing_ran(watch, "at load")
  t.eq(E.raised, 0, "at load: nothing raised")
end

do
  -- The same fixture, the same bytes, this session's stamp: the chunk
  -- compiles and runs, so what stopped the one above was the fence and
  -- not the planting.
  local env, host, box, watch = prepared("hook")
  planted(env, box, "hook", "0000000001-abcd.req",
    "op: eval\nfor: " .. STAMP .. "\nstate: hook\n\n" .. KILL)
  local E = load(env)
  host.callbacks.onSimulationFrame()
  t.eq(watch.compiled, 1, "at load, addressed: the chunk was compiled")
  t.eq(watch.ran, 1, "at load, addressed: and ran")
  local order, v = read(E, "0000000001-abcd")
  t.eq(v.status, "error", "at load, addressed: the chunk this suite hands back raises")
  t.eq(v.stage, "run", "at load, addressed: under stage run, which is the chunk's own")
  t.eq(order[#order - 2], "stage", "at load, addressed: and the reply is an eval's, not a fence's")
end

--------------------------------------------------------------------------------
-- The stamp alone decides
--------------------------------------------------------------------------------

do
  -- Every one of these would be refused for a second reason if the fence
  -- let it through, and each is `stale-session` instead, because the fence
  -- is judged before an op is looked for.
  local E, env, host, watch = loaded("hook")
  request(E, "1-a.req", "for: 999-7\n\n" .. KILL)
  request(E, "1-b.req", "op: sabotage\nfor: 999-7\n\n" .. KILL)
  request(E, "1-c.req", "op: eval\nfor: 999-7\nstate: hook\n\n")
  request(E, "1-d.req", "op: eval\nfor: 999-7\nstate: nowhere\n\n" .. KILL)
  request(E, "1-e.req", "op: ping\nfor: 999-7\n\n")
  -- The one that would leave this state. Every other request in this suite
  -- names `hook`, whose carrier is `local`, so a fence that stopped fencing
  -- would be read by the compile count; this one names a state the hook
  -- host reaches through `net.dostring_in`, where nothing is compiled here
  -- and the send count is the only one that can see it go.
  request(E, "1-f.req", "op: eval\nfor: 999-7\nstate: mission\n\n" .. KILL)
  -- And the one two hops out. `missionscripting` is the only state whose
  -- carrier is `a_do_script`, reached through `net.dostring_in` into
  -- `mission` first; `1-f`'s single hop stands in for every other state the
  -- hook host sends across, and nothing stands in for this one. A fence
  -- exempting it by name would be read by the same send count, which
  -- watches the first hop.
  request(E, "1-g.req", "op: eval\nfor: 999-7\nstate: missionscripting\n\n" .. KILL)
  host.callbacks.onSimulationFrame()
  fenced(E, "1-a", "999-7", "no op")
  fenced(E, "1-b", "999-7", "an unknown op")
  fenced(E, "1-c", "999-7", "an eval with no body")
  fenced(E, "1-d", "999-7", "a state no host serves")
  fenced(E, "1-e", "999-7", "a ping, which would have been answered")
  fenced(E, "1-f", "999-7", "a state across net.dostring_in")
  fenced(E, "1-g", "999-7", "a state two hops out, across a_do_script")
  t.eq(entries(env, E.req), "", "the stamp alone: every one was taken")
  nothing_ran(watch, "the stamp alone")
end

do
  -- The counter the case above holds at zero, moving. The same bytes for
  -- this session cross into `mission` and nothing is compiled or run in
  -- this state, so "nothing was sent to a state" is a count this suite has
  -- watched fill and not an absence nobody could tell from a dead wire.
  local E, _, host, watch = loaded("hook")
  request(E, "1-h.req", "op: eval\nfor: " .. STAMP .. "\nstate: mission\n\n" .. KILL)
  host.callbacks.onSimulationFrame()
  t.eq(watch.sent, 1, "addressed, across dostring_in: the chunk crossed into the state")
  t.eq(watch.compiled, 0, "addressed, across dostring_in: with nothing compiled here")
  t.eq(watch.ran, 0, "addressed, across dostring_in: and nothing run here")
  local _, v = read(E, "1-h")
  t.eq(v.stage, "dostring_in", "addressed, across dostring_in: the reply is the carrier's")
end

do
  -- The other side of the slot the echo rides in. `for` is added to a
  -- header list every refusal in `admit` can reach, so a request for this
  -- session, refused for one of the second reasons above, must carry the
  -- eight and no more: the echo belongs to `stale-session` alone, and a
  -- `bad-request` that grew one would be a reply shape no wire describes.
  --
  -- The foreign request goes first, and its name sorts first, so the two
  -- below are judged in a session where the slot has already been filled
  -- once. Without it the slot is nil because nothing ever wrote it, and
  -- the two checks pass for a reason that is not the property: a slot
  -- hoisted out of `admit` and left set — the refactor this guards against
  -- — would hang this session's stamp on the next refusal and nothing here
  -- would say so.
  local E, _, host, watch = loaded("hook")
  kill(E, "1-i.req", "999-7")
  request(E, "1-j.req", "for: " .. STAMP .. "\n\n" .. KILL)
  request(E, "1-k.req", "op: eval\nfor: " .. STAMP .. "\nstate: hook\n\n")
  host.callbacks.onSimulationFrame()
  fenced(E, "1-i", "999-7", "the refusal that fills the slot")
  local order, v = read(E, "1-j")
  fields(order, HEAD, "addressed, no op")
  t.eq(v.status, "bad-request", "addressed, no op: refused for the op it does not name")
  order, v = read(E, "1-k")
  fields(order, HEAD, "addressed, an eval with no body")
  t.eq(v.status, "bad-request", "addressed, an eval with no body: refused for the body")
  nothing_ran(watch, "addressed and refused")
end

do
  -- Where the stamp stops deciding: the envelope. A request whose headers
  -- do not parse has no `for` for the fence to read, so it is refused as a
  -- bad envelope though the stamp it spells is another session's. The
  -- comment above `admit` scopes the fence to past the envelope and this
  -- holds it there, so nobody reads "a foreign request that is also
  -- malformed reads as foreign" as covering bytes that are not a request.
  local E, _, host, watch = loaded("hook")
  request(E, "1-l.req", "op: eval\nfor: 999-7\nfor: 999-7\nstate: hook\n\n" .. KILL)
  request(E, "1-m.req", "op: eval\nfor: 999-7\nstate: hook\n")
  host.callbacks.onSimulationFrame()
  local order, v, body = read(E, "1-l")
  fields(order, HEAD, "a repeated header")
  t.eq(v.status, "bad-request", "a repeated header: the envelope is refused, not the stamp")
  t.check(body:find("repeated", 1, true), "a repeated header: and the message says which: " .. body)
  order, v, body = read(E, "1-m")
  fields(order, HEAD, "no blank line")
  t.eq(v.status, "bad-request", "no blank line: the envelope is refused, not the stamp")
  t.check(body:find("the headers never end", 1, true), "no blank line: and the message says why: " .. body)
  nothing_ran(watch, "an envelope that does not parse")
end

--------------------------------------------------------------------------------
-- The comparison is exact
--------------------------------------------------------------------------------

do
  local E, _, host, watch = loaded("hook")
  kill(E, "2-a.req", STAMP .. " ")
  kill(E, "2-b.req", STAMP .. "0")
  kill(E, "2-c.req", STAMP:sub(1, #STAMP - 1))
  kill(E, "2-d.req", "-" .. PID)
  kill(E, "2-e.req", CLOCK .. "-")
  host.callbacks.onSimulationFrame()
  fenced(E, "2-a", STAMP .. " ", "a blank after the stamp")
  fenced(E, "2-b", STAMP .. "0", "one byte more than the stamp")
  fenced(E, "2-c", STAMP:sub(1, #STAMP - 1), "a prefix of the stamp")
  fenced(E, "2-d", "-" .. PID, "the pid alone")
  fenced(E, "2-e", CLOCK .. "-", "the time alone")
  nothing_ran(watch, "the comparison is exact")

  -- The stamp itself is not fenced, so the comparison refuses something.
  kill(E, "2-f.req", STAMP)
  host.callbacks.onSimulationFrame()
  local order, v = read(E, "2-f")
  t.eq(v.status, "error", "the stamp itself: the chunk reached the carrier")
  t.eq(v.stage, "run", "the stamp itself: and raised there")
  t.eq(order[#order], "budget", "the stamp itself: the reply is an eval's")
  t.eq(watch.compiled, 1, "the stamp itself: one chunk was compiled")
  t.eq(watch.ran, 1, "the stamp itself: and ran")

  -- The name is the parser's, not the client's: it lowers a header name, so
  -- a request that wrote `For` is judged like any other and the echo comes
  -- back under the name the executor spells, which is the one a client's
  -- parser is reading for.
  request(E, "2-g.req", "op: ping\nFor: 999-7\n\n")
  host.callbacks.onSimulationFrame()
  fenced(E, "2-g", "999-7", "a capital For")
end

--------------------------------------------------------------------------------
-- A stamp longer than a message should carry
--------------------------------------------------------------------------------

do
  local E, _, host, watch = loaded("hook")
  local long = string.rep("9", 100)
  kill(E, "3-a.req", long)
  host.callbacks.onSimulationFrame()
  local _, v, body = read(E, "3-a")
  t.eq(v["for"], long, "long: the header carries the stamp whole")
  t.eq(body, refusal(long), "long: and the message excerpts it")
  t.eq(body:find(string.rep("9", 80) .. "...", 1, true), 6, "long: at eighty bytes and three dots")
  nothing_ran(watch, "long")
end

do
  -- The byte the rule turns on. Eighty is not longer than eighty, so the
  -- message carries the stamp whole; eighty-one is, so it is cut. Every
  -- other stamp this suite sends is three bytes or a hundred, where a cut
  -- at eighty and a cut past it agree, and so is the stand-in's: without
  -- these two the executor and the Rust stand-in could read the rule
  -- differently and no test on either side would say so. The messages are
  -- spelt out here rather than built by `refusal`, whose copy of the rule
  -- would move with the mistake.
  local E, _, host, watch = loaded("hook")
  local eighty = string.rep("8", 80)
  local past = string.rep("7", 81)
  kill(E, "3-b.req", eighty)
  kill(E, "3-c.req", past)
  host.callbacks.onSimulationFrame()

  local _, v, body = read(E, "3-b")
  t.eq(v["for"], eighty, "eighty: the header carries the stamp whole")
  t.eq(body, "for: " .. eighty .. " is not this session's stamp, " .. STAMP
    .. ": the request was written for another session and was not run",
    "eighty: and so does the message, uncut")
  t.check(not body:find("...", 1, true), "eighty: with no dots anywhere in it: " .. body)

  _, v, body = read(E, "3-c")
  t.eq(v["for"], past, "eighty-one: the header carries the stamp whole")
  t.eq(body, "for: " .. string.rep("7", 80) .. "... is not this session's stamp, " .. STAMP
    .. ": the request was written for another session and was not run",
    "eighty-one: and the message is cut to eighty and three dots")
  nothing_ran(watch, "the byte the cut turns on")
end

do
  -- The echo is uncapped, which ADR 0006 settled against capping it at the
  -- two hundred bytes a `chunkname` is capped at. Nothing shorter than that
  -- can say so: every other stamp this suite sends, and the stand-in's, is a
  -- hundred bytes or fewer, which a cap at two hundred would leave whole. A
  -- stamp of three hundred is the check that reddens for the cap the record
  -- rejected, so the alternative cannot be taken quietly.
  local E, _, host, watch = loaded("hook")
  local uncapped = string.rep("6", 300)
  kill(E, "3-d.req", uncapped)
  host.callbacks.onSimulationFrame()
  local order, v, body = read(E, "3-d")
  fields(order, FENCED, "past the chunkname cap")
  t.eq(#v["for"], 300, "past the chunkname cap: the header carries all three hundred bytes")
  t.eq(v["for"], uncapped, "past the chunkname cap: and they are the ones the request named")
  t.eq(body, refusal(uncapped), "past the chunkname cap: while the message still excerpts at eighty")
  nothing_ran(watch, "past the chunkname cap")
end

--------------------------------------------------------------------------------
-- The size check comes first
--------------------------------------------------------------------------------

do
  -- A request over the limit is refused without being opened, so its `for`
  -- is never read and the refusal is `bad-request`, not the fence's. The
  -- order matters: the fence must never be the reason to open 300 000
  -- bytes, which is what the fixture below writes against a limit of
  -- 262,144.
  local E, _, host, watch = loaded("hook")
  request(E, "4-a.req", "op: eval\nfor: 999-7\nstate: hook\n\n" .. string.rep("x", 300000))
  host.callbacks.onSimulationFrame()
  local order, v, body = read(E, "4-a")
  fields(order, HEAD, "oversize")
  t.eq(v.status, "bad-request", "oversize: the size refusal, not the fence's")
  t.check(body:find("was not read", 1, true), "oversize: and it says the bytes were never read: " .. body)
  nothing_ran(watch, "oversize")
end

--------------------------------------------------------------------------------
-- One foreign request among this session's own
--------------------------------------------------------------------------------

do
  local E, env, host, watch = loaded("hook")
  request(E, "5-a.req", "op: ping\nfor: " .. STAMP .. "\n\n")
  kill(E, "5-b.req", "999-7")
  request(E, "5-c.req", "op: ping\nfor: " .. STAMP .. "\n\n")
  host.callbacks.onSimulationFrame()
  t.eq(entries(env, E.req), "", "among its own: every request was taken")
  t.eq(entries(env, E.res), "5-a.res 5-b.res 5-c.res", "among its own: and every one answered")
  local _, v, body = read(E, "5-a")
  t.eq(v.status .. " " .. body, "ok pong", "among its own: the one before it is answered")
  fenced(E, "5-b", "999-7", "among its own")
  _, v, body = read(E, "5-c")
  t.eq(v.status .. " " .. body, "ok pong", "among its own: and the one after it, on the same tick")
  t.eq(v.tick, "1", "among its own: the fence spent no tick of its own")
  nothing_ran(watch, "among its own")
end

--------------------------------------------------------------------------------
-- The export host
--------------------------------------------------------------------------------

do
  local E, env, _, watch = loaded("export")
  t.eq(E.host, "export", "export: the host is the export state's")
  -- The request names `export` and not the `kill` helper's `hook`, because
  -- this host serves its own state alone: a request naming `hook` is
  -- `unsupported` before a carrier is chosen, so with the fence taken out
  -- nothing would have compiled here and the control below would hold.
  request(E, "6-a.req", "op: eval\nfor: 999-7\nstate: export\n\n" .. KILL)
  rawget(env, "LuaExportAfterNextFrame")()
  t.eq(entries(env, E.req), "", "export: the request was taken")
  local order, v, body = read(E, "6-a")
  fields(order, FENCED, "export")
  t.eq(v.status, "stale-session", "export: the status is stale-session")
  t.eq(v.host, "export", "export: answered by the export host")
  t.eq(v["for"], "999-7", "export: echoing the stamp the request named")
  t.eq(body, refusal("999-7"), "export: with the same message the hook host writes")
  nothing_ran(watch, "export")
  t.eq(E.raised, 0, "export: nothing raised")
end
