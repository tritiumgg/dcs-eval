-- The driver half of the round-trip control: the shipped executor, loaded
-- under the reference interpreter and ticked until a request another
-- process put in front of it is answered. The other half is
-- `crates/dcs-eval/src/e2e.rs`, which spawns this suite through the runner,
-- reads the handshake, sends a `ping` through the client's own `send`, and
-- parses the reply. `executor/interop` plants its own requests before one
-- frame; this is the one place the client's request meets a live tick.
--
-- Where the files land. As in `executor/interop`: when `DCS_EVAL_E2E` names
-- a directory, that directory is the box, no sandbox is made and nothing is
-- swept; unset, the suite runs over a sandbox like any other. The hazard
-- is worse here than there. A shell that exports the variable makes the
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

local host = { writedir = box .. SAVED, tempdir = box .. TEMP }
local env = t.state("hook", host)
t.load_executor(env)()
local E = rawget(env, NAME)
t.eq(type(E), "table", "the executor loaded over the box")
t.eq(host.log, nil, "and nothing reached dcs.log: no refusal, no fallback")

local frame = host.callbacks.onSimulationFrame
local reply = E.res .. "\\" .. ID .. ".res"

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
-- The executor names the arm file and never makes or removes it today, so
-- the one on the disk is the client's. When the dormant shape is built the
-- executor removes it on the way to sleep, and this check moves.
t.eq(env.lfs.attributes(E.arm, "mode"), "file", "the client armed the session")
t.eq(env.lfs.attributes(E.handshake, "mode"), "file", "the handshake is on the disk")
