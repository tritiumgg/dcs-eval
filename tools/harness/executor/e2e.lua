-- The driver half of the round-trip control: the shipped executor, loaded
-- under the reference interpreter and ticked until a request another
-- process put in front of it is answered. The other half is
-- `crates/dcs-eval/src/e2e.rs`, which spawns this suite through the runner,
-- reads the handshake, sends a `ping` through the client's own `send`, and
-- parses the reply. `executor/interop` plants its own requests before one
-- frame; this is the one place the client's request meets a live tick.
--
-- Where the files land. As in `executor/interop`: when `DCS_EVAL_E2E` names
-- a directory, that directory is the box, no sandbox is made and the runner
-- removes nothing; unset, the suite runs over a sandbox like any other. The
-- hazard is worse here than there. A shell that exports the variable makes the
-- full harness wait for a client that never comes, for the whole deadline
-- below, and then fail: the check that the directory exists is the only
-- guard, and this comment the warning.
--
-- Who sends. With a box named, the client does, from another process, at a
-- moment this suite cannot see; so the suite ticks until the reply is on
-- the disk or the deadline passes. With no box named there is no client,
-- and the suite is its own: before the third frame it writes the request
-- under its final name and touches the arm file, through the runner's own
-- `io`. That keeps the suite non-empty under the plain harness run and
-- proves the loop takes a request that was not there at the first frame.
-- It is not the cross-process claim: nothing is renamed into place and no
-- other process is involved, and a green run in this mode says only that.
--
-- The restart. With a box named and `DCS_EVAL_E2E_RESTART` set to two pids,
-- the suite is the other half of a round trip that never comes back. The
-- first session loads under the first pid and is never ticked; once the
-- client has sent and dropped `restart` at the box's root, a second
-- session loads in a state of its own over the same box, under the second
-- pid, as a DCS launched again would. It sweeps the first session's
-- directory, the request and the arm file with it, and publishes its own
-- handshake under the one name a client re-reads. Then it ticks a few
-- frames to show it was handed nothing. The client's `wait` on the first
-- session runs after this suite has ended, and its verdict is the Rust
-- side's claim. The sentinel, rather than the request's own name, is what
-- the suite waits for, because `send` ensures the arm file after the
-- request lands, and a sweep in that gap races the client for the
-- directory.
--
-- The deadline is wall-clock seconds from after the load, counted with
-- `os.time`, whose granularity is nothing against the budget. It is shorter
-- than the Rust side's on purpose: when nothing arrives, this suite gives
-- up first and its failure line, with the executor's counters in it, is
-- what the Rust side reports. A frame is not free here: the model's `lfs`
-- runs `cmd.exe` for every listing, so the loop asks for the reply through
-- the runner's `io.open` on the one name both sides agree on, which costs
-- nothing, and lists nothing until the loop has ended.
--
-- No envelope is read here, for the reason `executor/interop` gives:
-- reading is the client's claim, and a check here that read the reply
-- would prove this suite's reader, not the client's.
local t = ...

local NAME = "DcsEvalExecutor"
local ID = "0000000001-ping"
local DEADLINE_S = 20
local PLANT_AT = 3
-- The frames the second session runs after a restart: few, because every
-- listing the model's `lfs` makes spawns `cmd.exe`.
local TICKS = 3

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

-- One file, written through the runner's own `io`.
local function write(path, content)
  local fh = assert(io.open(path, "wb"))
  fh:write(content)
  fh:close()
end

-- Whether a file is on the disk under that name, through the runner's own
-- `io`: a reply is published by rename, so one that opens is whole.
local function present(path)
  local fh = io.open(path, "rb")
  if fh then
    fh:close()
    return true
  end
  return false
end

-- The box: the directory the caller named, which must exist, or a sandbox.
-- A rename onto itself is the runner's own test for "exists".
local given = os.getenv("DCS_EVAL_E2E")
local box
if given then
  if not os.rename(given, given) then
    error("harness: DCS_EVAL_E2E names " .. given .. ", which does not exist", 0)
  end
  box = given
else
  box = t.sandbox()
end

-- The two pids a restart runs under, the first session's and the second's.
local restart = os.getenv("DCS_EVAL_E2E_RESTART")
local old, new
if restart then
  if not given then
    error("harness: DCS_EVAL_E2E_RESTART is set, but DCS_EVAL_E2E names no box", 0)
  end
  old, new = restart:match("^(%d+) (%d+)$")
  if not old then
    error("harness: DCS_EVAL_E2E_RESTART is \"" .. restart .. "\", not two pids", 0)
  end
end

local host = { writedir = box .. SAVED, tempdir = box .. TEMP, pid = tonumber(old) }
local env = t.state("hook", host)
t.load_executor(env)()
local E = rawget(env, NAME)
t.eq(type(E), "table", "the executor loaded over the box")
t.eq(host.log, nil, "and nothing reached dcs.log: no refusal, no fallback")

local frame = host.callbacks.onSimulationFrame
local reply = E.res .. "\\" .. ID .. ".res"

if restart then
  local deadline = os.time() + DEADLINE_S
  local signalled = present(box .. "\\restart")
  while not signalled and os.time() < deadline do
    signalled = present(box .. "\\restart")
  end
  t.check(signalled, "restart: the client said it had sent, within " .. DEADLINE_S .. " s")
  t.check(present(E.req .. "\\" .. ID .. ".req"), "restart: the request is in the first session's req")
  t.eq(env.lfs.attributes(E.arm, "mode"), "file", "restart: the client armed the first session")
  t.eq(E.tick, 0, "restart: the first session never ran a frame")

  local host2 = { writedir = host.writedir, tempdir = host.tempdir, pid = tonumber(new) }
  local env2 = t.state("hook", host2)
  t.load_executor(env2)()
  local B = rawget(env2, NAME)
  t.eq(type(B), "table", "restart: the second session loaded over the same box")
  t.eq(host2.log, nil, "restart: nothing reached dcs.log, the sweep included")
  t.check(B.stamp ~= E.stamp,
    "restart: the second session has a stamp of its own (" .. E.stamp .. ", " .. B.stamp .. ")")
  t.eq(B.handshake, E.handshake,
    "restart: its handshake replaces the first's, under the one name a client re-reads")
  t.eq(B.swept, 1, "restart: the first session was swept")
  t.eq(env2.lfs.attributes(E.session, "mode"), nil,
    "restart: the first session's directory is gone, request and arm file with it")

  for _ = 1, TICKS do
    host2.callbacks.onSimulationFrame()
  end
  t.eq(entries(env2, B.req), "", "restart: the second session was handed nothing")
  t.eq(entries(env2, B.res), "", "restart: and answered nothing")
  t.eq(B.raised, 0, "restart: nothing raised")
  t.eq(B.unpublished, 0, "restart: nothing failed to publish")
  return
end

-- The suite as its own client, in the order the client sends: the request
-- under its final name, then the arm file.
local function plant()
  write(E.req .. "\\" .. ID .. ".req", "op: ping\nfor: " .. E.stamp .. "\n\n")
  write(E.arm, "")
end

local deadline = os.time() + DEADLINE_S
local answered = false
while os.time() < deadline do
  if not given and E.tick + 1 == PLANT_AT then
    plant()
  end
  frame()
  if present(reply) then
    answered = true
    break
  end
end

-- The counters go into the failure line, because "no reply" alone does not
-- say whether the request was never listed, was taken and not answered, or
-- was answered somewhere else.
t.check(answered, "a reply to " .. ID .. " was published within " .. DEADLINE_S .. " s"
  .. " (tick " .. E.tick .. ", raised " .. E.raised .. ", unpublished " .. E.unpublished
  .. ", req [" .. entries(env, E.req) .. "], res [" .. entries(env, E.res) .. "])")
t.check(E.tick >= 1, "at least one frame ran")
t.eq(entries(env, E.req), "", "the request was taken")
t.eq(entries(env, E.res), ID .. ".res", "the reply is under res by its final name, no .tmp")
t.eq(E.raised, 0, "nothing raised")
t.eq(E.unpublished, 0, "nothing failed to publish")
-- The arm file on the disk is the client's: a client creates it, and the
-- executor removes it only on the way back to sleep, which is a whole quiet
-- period away from a suite that ends on the reply. That the executor woke on
-- it at all is what this suite now shows end to end, the load being asleep.
t.eq(env.lfs.attributes(E.arm, "mode"), "file", "the client armed the session")
t.eq(env.lfs.attributes(E.handshake, "mode"), "file", "the handshake is on the disk")
