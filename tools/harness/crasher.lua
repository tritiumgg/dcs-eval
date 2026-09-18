-- The supervisor's reader, modelled: which request was running when DCS
-- died, read out of the markers the executor wrote before and after each
-- one.
--
-- The supervisor itself is the consuming project's and reads the markers
-- with the rule its probe progress files already used — the last opening
-- marker with no closing one. Nothing here ships. This is the oracle the
-- executor's grammar is judged against, kept apart from the executor so
-- that a suite is not reading the rule off the code it is judging, and it
-- is what a synthetic unbalanced file is fed to.
--
-- The rule, in full:
--
--   * A record is `B|<id>|…` or `O|<id>|…`, at the start of a line or at
--     the end of one that names the executor, which is where `log.write`
--     puts the markers of a crossing into `missionscripting` inside
--     `dcs.log`. Everything else in the line, and every line that
--     carries neither, is passed over: the events log also holds a load
--     banner, and `dcs.log` holds everything DCS has to say.
--   * An opening record opens its id. A closing record closes the newest
--     record open under that id, and one that closes nothing is ignored,
--     because a generation of the log can begin between the two halves of
--     a request.
--   * The killer is the id of the record left open last. Nothing open is
--     nobody: every request that started also finished.
local NAME = "DcsEvalExecutor"

-- The kind and the id of the record in `line`, or nil. Anchored twice on
-- purpose: a marker in the events log is the whole line, and one in
-- `dcs.log` is the tail of a line DCS rendered around it, whose shape is
-- DCS's own — a timestamp, a level, the subsystem `log.write` was given,
-- and, in some builds, the thread beside it. Reading the marker as what
-- follows the last colon-space of a line naming the executor holds under
-- any of those without this file claiming to know which.
local function record(line)
  local kind, id = line:match("^([BO])|([^|]*)|")
  if kind then
    return kind, id
  end
  if line:find(NAME, 1, true) then
    return line:match(".*: ([BO])|([^|]*)|")
  end
  return nil
end

-- The id of the request that was running when the log stopped, or nil.
-- `text` is a whole file: the events log, `dcs.log`, or the two read
-- together. A line keeps whatever ends it: both fields a record is read
-- for end at a separator, so a CR before the LF of a file written on
-- Windows falls outside them and nothing here strips one.
return function(text)
  local open = {}
  for line in (text .. "\n"):gmatch("([^\n]*)\n") do
    local kind, id = record(line)
    if kind == "B" then
      open[#open + 1] = id
    elseif kind == "O" then
      for i = #open, 1, -1 do
        if open[i] == id then
          table.remove(open, i)
          break
        end
      end
    end
  end
  return open[#open]
end
