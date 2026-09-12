-- The driver half of the interop control: the shipped executor, loaded and
-- driven under the reference interpreter, left on the disk for the Rust
-- client to read. The other half is `crates/dcs-eval/src/interop.rs`, which
-- spawns this suite through the runner and parses what it wrote. It is the
-- one place the two implementations of the protocol meet.
--
-- Where the files land. The runner takes suite names and nothing else, and
-- it sweeps every sandbox it made when the suite ends; cargo needs to name
-- where the bytes go and read them after the interpreter has exited. So
-- when `DCS_EVAL_INTEROP` names a directory, that directory is the box, no
-- sandbox is made and nothing is swept. When it is unset the suite runs as
-- any other, over a sandbox, and proves the same things to the harness. A
-- shell that exports the variable makes the full harness write there, once
-- per run, unswept: the check below that the directory exists is the only
-- guard, and this comment the warning.
--
-- What is planted. A `ping` and an `eval`, then one frame. The `eval`
-- carries a body because one without is refused before dispatch; it is
-- answered `unsupported` today, because the op is declared and not served,
-- and when it is served the answer is the model's `net.dostring_in`, which
-- runs nothing and answers empty. That is why `loadstring` and
-- `net.dostring_in` are the model's own here and not the failing ones
-- `executor/ping` installs: the empty answer is what the wire will carry,
-- and the reader must see it as empty.
--
-- What is proved here is only that the run went where the reader looks:
-- the load got past containment over this box, nothing reached `dcs.log`,
-- both requests were taken and both replies published under their final
-- names, and the handshake is on the disk. No envelope is read here on
-- purpose: reading them is the Rust half's claim, and a check here that
-- read them would prove the suite's own reader, not the client's.
local t = ...

local NAME = "DcsEvalExecutor"

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

-- One request under `E.req`, written through the runner's own `io`.
local function request(E, name, content)
  local fh = assert(io.open(E.req .. "\\" .. name, "wb"))
  fh:write(content)
  fh:close()
end

-- The box: the directory the caller named, which must exist, or a sandbox.
-- A rename onto itself is the runner's own test for "exists".
local given = os.getenv("DCS_EVAL_INTEROP")
local box
if given then
  if not os.rename(given, given) then
    error("harness: DCS_EVAL_INTEROP names " .. given .. ", which does not exist", 0)
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

request(E, "0000000001-ping.req", "op: ping\nfor: " .. E.stamp .. "\n\n")
request(E, "0000000002-eval.req", "op: eval\nfor: " .. E.stamp .. "\n\nreturn 1")
host.callbacks.onSimulationFrame()

t.eq(E.tick, 1, "one frame ran")
t.eq(entries(env, E.req), "", "both requests were taken")
t.eq(entries(env, E.res), "0000000001-ping.res 0000000002-eval.res",
  "both replies are under res by their final names, no .tmp")
t.eq(E.raised, 0, "nothing raised")
t.eq(E.unpublished, 0, "nothing failed to publish")
t.eq(env.lfs.attributes(E.handshake, "mode"), "file", "the handshake is on the disk")
