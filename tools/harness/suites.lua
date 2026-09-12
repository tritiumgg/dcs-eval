-- Every suite the harness runs, in order. A name is a file under this
-- directory without its `.lua`; a directory of suites is run by naming the
-- directory. A suite that is not here does not run, so the task that adds
-- one adds its line.
return {
  "selftest",
  "stubs",
  "executor/load",
  "executor/containment",
  "executor/session",
  "executor/framer",
  "executor/request",
  "executor/handshake",
  "executor/ping",
  "executor/interop",
}
