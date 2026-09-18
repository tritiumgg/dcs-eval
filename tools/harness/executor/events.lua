-- The events log: the file that names the request DCS died in, and the
-- markers a supervisor reads it out of, written around every dispatch
-- and, from inside `mission`, around every crossing into
-- `missionscripting`.
--
-- What the kill looks like from here. A request file is off the disk
-- before its chunk runs, so a chunk that takes the process with it leaves
-- nothing behind but what the executor had already written: the opening
-- marker, and no closing one. This suite does not kill the interpreter to
-- get that state; it reads the file at the moment the kill would happen,
-- from inside the chunk itself. The killing request's body opens the
-- events log, reads it and returns the bytes, so the reply carries the
-- file exactly as a killed session would have left it, written by the
-- executor and not by the suite. Those bytes are what the reader is fed.
--
-- The reader is `tools/harness/crasher.lua`, a model of the supervisor's
-- own rule — the last opening marker with no closing one — kept out of
-- this file so that the grammar is judged against something other than
-- the code that wrote it. The supervisor itself belongs to the consuming
-- project and nothing here ships it.
--
-- What is proved. The load rotates: the last generation becomes
-- `events.prev.log`, the new one opens with its banner, and a third load
-- leaves two files and not three, so the ceiling is two launches. A
-- generation that cannot be moved — on Windows, a file a reader holds
-- open — is named on the namespace and appended to rather than refused.
-- Every request the tick dispatches is bracketed: `B|<id>|<op>|<state>|
-- <stamp>` before it and `O|<id>|<status>|<cpu_ms>` after, the opening
-- marker on the disk before the reply and the closing one after it, with
-- the status and the cost the reply itself carried and not a second
-- reading of the clock, which a clock that moves between the two proves.
-- A `ping` names no state and leaves that field empty. A request refused
-- before dispatch — a bad envelope, a foreign stamp, no op — gets no pair
-- at all, because it never ran. The three fields a client spells are each
-- held to the grammar: a separator or a byte that is not printable ASCII
-- becomes `?`, and a value over eighty bytes is cut where every refusal
-- message cuts one — the id in both markers, where it is the only field
-- the client wrote. An op that frames no reply at all closes its pair
-- with two empty fields rather than leaving the request open, and empty
-- after a request that filled both rather than standing from it. A
-- crossing into `missionscripting` is marked from inside `mission`,
-- through `log.write` into `dcs.log`, with the same four opening fields
-- and an empty cost, the opening marker written before `a_do_script` is
-- called and the closing one after it answers; a mission that is not
-- loaded is marked too, and says `no-mission`, and a mission state with
-- no `log` in it is marked in neither direction and answers as it would
-- have. A line that cannot be written — the file refusing to open, or a
-- handle that opened refusing the line — is counted on the namespace and
-- the reply still goes out. And the reader names the killer: out of the
-- file as the kill left it, and out of synthetic files a session cannot
-- produce — a close that arrives out of order, a close with nothing
-- open, a line that merely quotes a marker, and a crossing's markers as
-- `dcs.log` renders them.
--
-- The mutations this suite exists to catch. Drop the opening marker and
-- the killer reads as the request before it. Drop the sanitisation, from
-- either marker or from any of the three fields, and what a client spelt
-- goes into the file whole: a `|` of its own, or a name of a hundred
-- bytes. Leave the rotation out and the second generation is the first
-- one grown. Write the closing marker of a crossing before it instead of
-- after and the bracket is not one. Take the guard off the `log` the
-- crossing marks through and a mission state without one answers `error`
-- at `stage: bridge` where it answered `ok`. Charge the closing marker
-- from a fresh clock and it disagrees with the reply beside it. Leave
-- the two fields it reads standing from one request to the next and an
-- op that frames no reply is closed on the figures of the one before it.
-- In the reader: close a record by position rather than by id and an
-- out-of-order close names the wrong request; answer the first record
-- left open rather than the last and a file with one live request names
-- one that finished.
local t = ...

local NAME = "DcsEvalExecutor"

local SAVED = [[\Saved Games\DCS\]]
local TEMP = [[\Temp\DCS\]]

-- The clock every load here takes, so a stamp is known before the
-- executor mints it: `1000-<pid>`.
local CLOCK = 1000

-- The supervisor's rule, modelled apart from this suite.
local killer = dofile(t.root .. "/tools/harness/crasher.lua")

-- A hook state over `box` with the clock frozen and `pid` in the stamp,
-- loaded. Returns the namespace, the state, the host and the frame.
local function loaded(box, pid, host)
  host = host or {}
  host.writedir = box .. SAVED
  host.tempdir = box .. TEMP
  host.pid = pid
  local env = t.state("hook", host)
  env.os.time = function()
    return CLOCK
  end
  t.load_executor(env)()
  local E = rawget(env, NAME)
  t.eq(type(E), "table", "pid " .. pid .. ": the executor loaded")
  return E, env, host, host.callbacks.onSimulationFrame
end

-- A whole file through the runner's own `io`, or nil where there is none.
local function slurp(path)
  local fh = io.open(path, "rb")
  if not fh then
    return nil
  end
  local bytes = fh:read("*a")
  fh:close()
  return bytes
end

-- The lines of a file, without their newlines.
local function lines(text)
  local out = {}
  for line in (text or ""):gmatch("([^\n]*)\n") do
    out[#out + 1] = line
  end
  return out
end

-- The events log as lines.
local function events(E)
  return lines(slurp(E.events))
end

-- The entries of a directory as the model lists them, sorted.
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

-- One request under `E.req`, written through the runner's own `io`.
local function request(E, name, content)
  local fh = assert(io.open(E.req .. "\\" .. name, "wb"))
  fh:write(content)
  fh:close()
end

-- A reply read back: its headers by name, and its body.
local function reply(E, id)
  local bytes = assert(slurp(E.res .. "\\" .. id .. ".res"), id .. ": no reply on the disk")
  local blank = assert(bytes:find("\n\n", 1, true), id .. ": the reply has a blank line ending its headers")
  local v = {}
  for line in bytes:sub(1, blank):gmatch("([^\n]*)\n") do
    local name, value = line:match("^([A-Za-z0-9_%-]+): (.*)$")
    if name then
      v[name] = value
    end
  end
  return v, bytes:sub(blank + 2)
end

-- A `ping` for the session, under `id`.
local function ping(E, id)
  request(E, id .. ".req", "op: ping\nfor: " .. E.stamp .. "\n\n")
end

-- An `eval` in the hook state for the session, under `id`.
local function eval(E, id, body)
  request(E, id .. ".req", "op: eval\nfor: " .. E.stamp .. "\nstate: hook\n\n" .. body)
end

-- A chunk that returns the events log as it stands while it runs: what a
-- request that killed the process leaves on the disk.
local function reads_the_log(E)
  return 'local fh = assert(io.open([[' .. E.events .. ']], "rb")) '
    .. 'local bytes = fh:read("*a") fh:close() return bytes'
end

--------------------------------------------------------------------------------
-- The load: a banner, and one generation kept
--------------------------------------------------------------------------------

do
  local box = t.sandbox()
  local E, env = loaded(box, 7)
  t.eq(E.events, E.output .. [[\events.log]], "load: the events log is named under the output")
  t.eq(events(E)[1], "load|1000-7|hook|2", "load: and opens with the banner of this session")
  t.eq(#events(E), 1, "load: which is the whole of it")
  t.eq(E.unrecorded, 0, "load: nothing failed to reach it")
  t.eq(E.events_left, nil, "load: the first launch had nothing to rotate")

  local second = loaded(box, 8)
  t.eq(entries(env, E.output), "events.log events.prev.log executor.txt",
    "rotate: the output holds one generation and the one before it")
  t.eq(slurp(second.events), "load|1000-8|hook|2\n", "rotate: the new generation is this session's banner")
  t.eq(slurp(E.output .. [[\events.prev.log]]), "load|1000-7|hook|2\n",
    "rotate: and the generation before it is the launch before it")

  local third = loaded(box, 9)
  t.eq(entries(env, E.output), "events.log events.prev.log executor.txt",
    "rotate: a third launch leaves two generations still")
  t.eq(slurp(third.events), "load|1000-9|hook|2\n", "rotate: the third is this session's")
  t.eq(slurp(E.output .. [[\events.prev.log]]), "load|1000-8|hook|2\n",
    "rotate: the second is behind it, and the first is gone")
end

-- A generation a reader holds open cannot be moved on Windows. The load
-- says so on the namespace and appends to what is there.
do
  local box = t.sandbox()
  local E = loaded(box, 7)
  local held = assert(io.open(E.events, "rb"))
  local again = loaded(box, 8)
  t.check(again.events_left and again.events_left:find("events.log", 1, true),
    "held: the load names the generation it could not move: " .. tostring(again.events_left))
  t.eq(slurp(again.events), "load|1000-7|hook|2\nload|1000-8|hook|2\n",
    "held: and appends its banner to the generation that stayed")
  t.eq(slurp(E.output .. [[\events.prev.log]]), nil,
    "held: and the first launch had no generation behind it to move")
  held:close()
end

-- A rotation that fails with a generation already behind it loses that
-- generation: the older one is removed before the rename onto it, which
-- is the order Windows needs, so a rename that then fails has nothing to
-- put back. The pair still holds two launches — the one that stayed and
-- the one appending to it — so the ceiling is what it was.
do
  local box = t.sandbox()
  local E = loaded(box, 7)
  local second = loaded(box, 8)
  local held = assert(io.open(second.events, "rb"))
  local third = loaded(box, 9)
  t.check(third.events_left and third.events_left:find("events.log", 1, true),
    "gone: the load names the generation it could not move: " .. tostring(third.events_left))
  t.eq(slurp(E.output .. [[\events.prev.log]]), nil,
    "gone: the launch behind the held one went before the rename that failed")
  t.eq(slurp(third.events), "load|1000-8|hook|2\nload|1000-9|hook|2\n",
    "gone: and two launches are still what the pair holds, in the file that stayed")
  held:close()
end

--------------------------------------------------------------------------------
-- A request, bracketed
--------------------------------------------------------------------------------

do
  local box = t.sandbox()
  local E, _, _, frame = loaded(box, 7)
  ping(E, "1-ping")
  frame()
  local v = reply(E, "1-ping")
  t.eq(v.status, "ok", "ping: answered")
  local log = events(E)
  t.eq(#log, 3, "ping: the banner and one pair")
  t.eq(log[2], "B|1-ping|ping||1000-7", "ping: the opening marker, with no state to name")
  t.eq(log[3], "O|1-ping|ok|" .. v.cpu_ms, "ping: and the closing one, with the reply's own status and cost")

  eval(E, "2-eval", "return 1 + 1")
  frame()
  v = reply(E, "2-eval")
  t.eq(v.status, "ok", "eval: answered")
  log = events(E)
  t.eq(log[4], "B|2-eval|eval|hook|1000-7", "eval: the opening marker names the state the request asked for")
  t.eq(log[5], "O|2-eval|ok|" .. v.cpu_ms, "eval: and the closing one the status it got")

  -- Three in one frame are three pairs, in the order they were answered.
  ping(E, "3-c")
  ping(E, "3-a")
  ping(E, "3-b")
  frame()
  log = events(E)
  t.eq(#log, 11, "order: three more pairs")
  local marks = {}
  for i = 6, 11 do
    marks[#marks + 1] = log[i]:match("^(.-|[^|]*)|")
  end
  t.eq(table.concat(marks, " "), "B|3-a O|3-a B|3-b O|3-b B|3-c O|3-c",
    "order: opened and closed one at a time, in name order")
  t.eq(killer(slurp(E.events)), nil, "order: nothing is left open, so nobody is the killer")
end

-- The cost in a closing marker is the reply's, not a later reading: a
-- clock that moves at every read would give two different figures.
do
  local box = t.sandbox()
  local E, env, _, frame = loaded(box, 7)
  local reads = 0
  env.os.clock = function()
    reads = reads + 1
    return reads * 0.005
  end
  ping(E, "1-clock")
  frame()
  local v = reply(E, "1-clock")
  t.eq(v.cpu_ms, "5.000", "clock: the reply is charged from the take to the framing")
  t.eq(events(E)[3], "O|1-clock|ok|5.000", "clock: and the closing marker carries that figure, not a fresh one")
  t.eq(reads, 3, "clock: three readings in the tick — the listing, the take, the framing — and the markers none")
end

--------------------------------------------------------------------------------
-- What gets no pair
--------------------------------------------------------------------------------

do
  local box = t.sandbox()
  local E, _, _, frame = loaded(box, 7)
  request(E, "1-envelope.req", "op ping\n\n")
  request(E, "2-foreign.req", "op: ping\nfor: 999-1\n\n")
  request(E, "3-noop.req", "for: " .. E.stamp .. "\n\n")
  request(E, "4-nobody.req", "op: eval\nfor: " .. E.stamp .. "\n\n")
  frame()
  t.eq(reply(E, "1-envelope").status, "bad-request", "refused: a bad envelope")
  t.eq(reply(E, "2-foreign").status, "stale-session", "refused: a foreign stamp")
  t.eq(reply(E, "3-noop").status, "bad-request", "refused: no op")
  t.eq(reply(E, "4-nobody").status, "bad-request", "refused: an eval with no body")
  t.eq(#events(E), 1, "refused: not one of the four reached an op, so not one is marked")

  ping(E, "5-ping")
  frame()
  t.eq(#events(E), 3, "refused: the request after them is marked as any other")
end

--------------------------------------------------------------------------------
-- The fields a client spells
--------------------------------------------------------------------------------

do
  local box = t.sandbox()
  local E, _, _, frame = loaded(box, 7)
  request(E, "1-pipe.req", "op: pi|ng\nfor: " .. E.stamp .. "\n\n")
  frame()
  local v = reply(E, "1-pipe")
  t.eq(v.status, "bad-request", "pipe: an op nothing serves is refused")
  local log = events(E)
  t.eq(log[2], "B|1-pipe|pi?ng||1000-7", "pipe: and the separator a client wrote is not one in the record")
  t.eq(log[3], "O|1-pipe|bad-request|" .. v.cpu_ms, "pipe: the pair closes on the refusal it got")

  request(E, "2-control.req", "op: eval\nfor: " .. E.stamp .. "\nstate: a\1b\n\nreturn 1")
  frame()
  t.eq(reply(E, "2-control").status, "bad-request", "control: a state of the wrong shape is refused")
  t.eq(events(E)[4], "B|2-control|eval|a?b|1000-7", "control: and the byte that is not printable is not in the record")

  local long = string.rep("x", 100)
  request(E, "3-long.req", "op: " .. long .. "\nfor: " .. E.stamp .. "\n\n")
  frame()
  t.eq(reply(E, "3-long").status, "bad-request", "long: an op of a hundred bytes is refused")
  t.eq(events(E)[6], "B|3-long|" .. string.rep("x", 80) .. "...||1000-7",
    "long: and the record carries eighty bytes and three dots")

  -- The id is the third field a client spells, and the only one in a
  -- closing marker. It is a filename, so a `|` or a byte below space
  -- cannot reach it on this host and length is the case that can: a name
  -- of a hundred bytes is cut as an op of a hundred bytes is.
  local wide = string.rep("y", 100)
  local cut = string.rep("y", 80) .. "..."
  request(E, wide .. ".req", "op: ping\nfor: " .. E.stamp .. "\n\n")
  frame()
  local answer = reply(E, wide)
  t.eq(answer.status, "ok", "id: a request named with a hundred bytes is answered")
  t.eq(events(E)[8], "B|" .. cut .. "|ping||1000-7",
    "id: and the opening marker names it at eighty bytes and three dots")
  t.eq(events(E)[9], "O|" .. cut .. "|ok|" .. answer.cpu_ms,
    "id: as does the closing one, where it is the only field the client wrote")
end

--------------------------------------------------------------------------------
-- An op that frames no reply
--------------------------------------------------------------------------------

-- The pair brackets the dispatch and not the reply, so an op that frames
-- none — one a driver hung on the published namespace — closes its record
-- with the two fields the reply would have carried left empty, rather
-- than leaving the request open and its id the killer.
--
-- A request that was answered goes first, so that the two fields are
-- known to have carried something before the hung op ran: a session that
-- did not clear them per request would close the hung one on the figures
-- of the request before it, which is the one fabrication this file exists
-- not to make.
do
  local box = t.sandbox()
  local E, _, _, frame = loaded(box, 7)
  rawset(E.ops, "hang", function() end)
  ping(E, "1-ping")
  frame()
  local answer = reply(E, "1-ping")
  t.eq(events(E)[3], "O|1-ping|ok|" .. answer.cpu_ms, "hang: the request before it was answered and charged")

  request(E, "2-hang.req", "op: hang\nfor: " .. E.stamp .. "\n\n")
  frame()
  local log = events(E)
  t.eq(#log, 5, "hang: the op ran and its record is a pair like any other")
  t.eq(log[4], "B|2-hang|hang||1000-7", "hang: opened as any other request is")
  t.eq(log[5], "O|2-hang||", "hang: and closed with no status and no cost, because none was framed")
  t.eq(killer(slurp(E.events)), nil, "hang: so the file names nobody, and not the request that answered nothing")
end

--------------------------------------------------------------------------------
-- The killer, out of the file as the kill leaves it
--------------------------------------------------------------------------------

do
  local box = t.sandbox()
  local E, _, _, frame = loaded(box, 7)
  ping(E, "1-before")
  frame()
  eval(E, "2-killer", reads_the_log(E))
  frame()
  local v, snapshot = reply(E, "2-killer")
  t.eq(v.status, "ok", "killer: the chunk read the log and came back")
  local seen = lines(snapshot)
  t.eq(seen[#seen], "B|2-killer|eval|hook|1000-7",
    "killer: what the chunk saw ends at its own opening marker, with no closing one")
  t.eq(killer(snapshot), "2-killer", "killer: which is what the reader names")
  t.eq(killer(slurp(E.events)), nil, "killer: the session that went on closed it, so the file names nobody")

  -- A second one, after the first is closed: the reader names the last
  -- request left open and not the first one that ever was.
  eval(E, "3-killer", reads_the_log(E))
  frame()
  local _, second = reply(E, "3-killer")
  t.eq(killer(second), "3-killer", "killer: the last opened, not the first")
  t.check(second:find("O|2-killer|ok|", 1, true), "killer: with the request before it closed in the same file")
end

--------------------------------------------------------------------------------
-- The crossing, marked from inside mission
--------------------------------------------------------------------------------

local function pack(...)
  return { n = select("#", ...), ... }
end

-- The first hop: `net.dostring_in` compiling what it was handed and
-- running it in the model `targets` names, as `executor/dostring` does.
local function carrier(env, targets)
  env.net.dostring_in = function(state, chunk)
    local target = targets[state]
    if not target then
      return "Invalid state name"
    end
    local fn = assert(loadstring(chunk, "=near"))
    setfenv(fn, target)
    return fn()
  end
end

-- The second hop: an `a_do_script` that runs the far chunk in `far` and
-- shifts what it returned by one, as DCS was measured to. Every call
-- leaves in `at` how many lines `mission` had written by then, so the
-- opening marker can be placed before the crossing rather than after it.
local function shifting(far, at, mission)
  return function(source, ...)
    at[#at + 1] = #(mission.log or {})
    local fn = assert(loadstring(source, "=far"))
    setfenv(fn, far)
    local returned = pack(fn(...))
    local out = { n = returned.n }
    for i = 1, returned.n - 1 do
      out[i + 1] = returned[i]
    end
    return unpack(out, 1, out.n)
  end
end

do
  local box = t.sandbox()
  local E, env, _, frame = loaded(box, 7)
  local mission = { mission_loaded = true }
  local state = t.state("mission", mission)
  local far = t.state("missionscripting", {})
  local at = {}
  state.a_do_script = shifting(far, at, mission)
  carrier(env, { mission = state })

  request(E, "1-cross.req", "op: eval\nfor: " .. E.stamp .. "\nstate: missionscripting\n\nreturn 7")
  frame()
  local v, body = reply(E, "1-cross")
  t.eq(v.status, "ok", "crossing: it answered")
  t.eq(body, "7", "crossing: with the far chunk's result")
  t.eq(#mission.log, 2, "crossing: two lines reached dcs.log from inside mission")
  t.eq(mission.log[1].subsystem, NAME, "crossing: written under the executor's name")
  t.eq(mission.log[1].level, state.log.INFO, "crossing: at INFO")
  t.eq(mission.log[1].state, "mission", "crossing: from the mission state, which has no io to write the events log with")
  t.eq(mission.log[1].message, "B|1-cross|eval|missionscripting|1000-7", "crossing: the opening marker, the executor's four fields")
  t.eq(mission.log[2].message, "O|1-cross|ok|", "crossing: the closing one, with the status and no cost to read")
  t.eq(at[1], 1, "crossing: the opening marker was written before a_do_script was called, and the closing one was not")

  -- The events log has its own pair for the same request, so a reader of
  -- either file names it.
  local log = events(E)
  t.eq(log[2], "B|1-cross|eval|missionscripting|1000-7", "crossing: the tick's own opening marker names the same request")
  t.eq(log[3], "O|1-cross|ok|" .. v.cpu_ms, "crossing: and its closing one carries the cost the mission state cannot")

  -- With no mission loaded the crossing is marked too.
  state.a_do_script = nil
  request(E, "2-menu.req", "op: eval\nfor: " .. E.stamp .. "\nstate: missionscripting\n\nreturn 7")
  frame()
  t.eq(reply(E, "2-menu").status, "no-mission", "menu: no mission is loaded")
  t.eq(mission.log[3].message, "B|2-menu|eval|missionscripting|1000-7", "menu: the crossing was still opened")
  t.eq(mission.log[4].message, "O|2-menu|no-mission|", "menu: and closed on what the near chunk answered")

  -- A mission state with no `log` is marked in neither direction and
  -- answers as it would have: a host global that is not there is read as
  -- every other one here is, and never raised on. Without that guard the
  -- raise is caught by the pcall around the crossing and the request
  -- comes back `bridge` instead of the answer it had.
  state.a_do_script = shifting(far, at, mission)
  rawset(state, "log", nil)
  request(E, "3-unlogged.req", "op: eval\nfor: " .. E.stamp .. "\nstate: missionscripting\n\nreturn 7")
  frame()
  local unlogged, result = reply(E, "3-unlogged")
  t.eq(unlogged.status, "ok", "unlogged: a mission state with no log answers as any other")
  t.eq(result, "7", "unlogged: with the far chunk's result still")
  t.eq(#mission.log, 4, "unlogged: and nothing more was written from inside mission")
end

--------------------------------------------------------------------------------
-- A line that cannot be written
--------------------------------------------------------------------------------

do
  local box = t.sandbox()
  local E, env, _, frame = loaded(box, 7)
  local open = env.io.open
  env.io.open = function(path, mode)
    if path == E.events then
      return nil, "Permission denied"
    end
    return open(path, mode)
  end
  ping(E, "1-lost")
  frame()
  t.eq(reply(E, "1-lost").status, "ok", "lost: the request is answered whatever the log did")
  t.eq(E.unrecorded, 2, "lost: both markers were counted")
  t.check(E.last_unrecorded and E.last_unrecorded:find("Permission denied", 1, true),
    "lost: with the reason kept: " .. tostring(E.last_unrecorded))
  t.eq(#events(E), 1, "lost: and the file still holds the banner alone")
end

-- A file that opens and will not take the line is the other half of the
-- same count: `unrecorded` is the lines that did not reach the file, not
-- the times it could not be opened, so a handle whose write fails is
-- counted and swallowed exactly as a refused open is.
do
  local box = t.sandbox()
  local E, env, _, frame = loaded(box, 7)
  local open = env.io.open
  env.io.open = function(path, mode)
    if path == E.events then
      return {
        write = function()
          return nil, "No space left on device"
        end,
        close = function()
          return true
        end,
      }
    end
    return open(path, mode)
  end
  ping(E, "1-full")
  frame()
  t.eq(reply(E, "1-full").status, "ok", "full: the request is answered whatever the write did")
  t.eq(E.unrecorded, 2, "full: both markers were counted, as a refused open is")
  t.check(E.last_unrecorded and E.last_unrecorded:find("No space left on device", 1, true),
    "full: with the reason the write gave: " .. tostring(E.last_unrecorded))
  t.eq(#events(E), 1, "full: and nothing of the pair reached the file")
end

--------------------------------------------------------------------------------
-- The reader, against files a session cannot produce
--------------------------------------------------------------------------------

do
  t.eq(killer(""), nil, "reader: an empty file names nobody")
  t.eq(killer("load|1000-7|hook|2\n"), nil, "reader: a session that handled nothing names nobody")
  t.eq(killer("B|a|ping||1000-7\nO|a|ok|0.000\n"), nil, "reader: a request that finished names nobody")
  t.eq(killer("B|a|ping||1000-7\n"), "a", "reader: one that did not is the killer")
  t.eq(killer("B|a|ping||1000-7\nO|a|ok|0.000\nB|b|eval|hook|1000-7\n"), "b",
    "reader: the last opened with no close, past the ones that closed")

  -- A close that arrives out of order closes its own record and not the
  -- newest one, which is the whole of the id field's work.
  t.eq(killer("B|a|ping||1000-7\nB|b|ping||1000-7\nO|a|ok|0.000\n"), "b",
    "reader: an out-of-order close closes the record it names")
  t.eq(killer("B|a|ping||1000-7\nB|b|ping||1000-7\nO|b|ok|0.000\n"), "a",
    "reader: and leaves the other one open")

  -- A generation can begin between the halves of a request, so a close
  -- with nothing open closes nothing and is not an error.
  t.eq(killer("O|a|ok|0.000\nB|b|ping||1000-7\n"), "b", "reader: a close with nothing open is passed over")

  -- Two generations read one after the other, each ending in a kill: the
  -- reader names the later one, which is the launch that just died.
  t.eq(killer("B|a|ping||1000-7\nload|1000-8|hook|2\nB|b|ping||1000-8\n"), "b",
    "reader: the last record left open, not the first")

  -- Lines that are not records: the banner, DCS's own, and one that
  -- merely quotes a marker without the executor's name on it.
  t.eq(killer("load|1000-7|hook|2\nB|a|ping||1000-7\nthe chunk printed B|zz|eval|hook|1000-7\nO|a|ok|0.000\n"), nil,
    "reader: a line that quotes a record is not one")

  -- A crossing's markers as dcs.log carries them: the message is the tail
  -- of a line DCS rendered, whose shape differs between builds.
  local rendered = "2026-09-17 12:00:00.000 INFO    " .. NAME .. ": B|d|eval|missionscripting|1000-7\n"
  t.eq(killer(rendered), "d", "reader: a marker read out of a rendered log line")
  local threaded = "2026-09-17 12:00:00.000 INFO    " .. NAME .. " (Main): B|e|eval|missionscripting|1000-7\n"
  t.eq(killer(threaded), "e", "reader: and out of one that names the thread beside the subsystem")
  t.eq(killer(threaded .. "2026-09-17 12:00:01.000 INFO    " .. NAME .. " (Main): O|e|ok|\n"), nil,
    "reader: which its own closing marker closes")
  t.eq(killer("2026-09-17 12:00:00.000 INFO    SCRIPTING: B|f|eval|hook|1000-7\n"), nil,
    "reader: a line that carries a marker under another name is not the executor's")

  -- CRLF is what a file written on Windows and read as text may carry.
  -- Nothing strips it: every field the reader takes ends at a separator,
  -- so the carriage return falls outside both of them, and a reader that
  -- read past one would be reading a field this grammar does not have.
  t.eq(killer("B|a|ping||1000-7\r\nO|a|ok|0.000\r\nB|b|ping||1000-7\r\n"), "b",
    "reader: a file with CRLF endings names what the same file with LF does")
end
