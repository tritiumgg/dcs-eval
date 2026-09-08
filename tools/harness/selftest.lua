-- The harness proving itself: a model that answers a name it does not carry
-- with a raise, at the line that read it, naming the path — rather than
-- with nil.
--
-- This is the one check the runner's own gate counts. Everything else the
-- runner promises is shown from outside, by driving it: the count it prints,
-- the exit for an empty suite, the exit for a name that matches nothing.
local t = ...

local DCS = t.strict("DCS", { getPause = function() return false end })

t.raises(function()
  return DCS.getPuase()
end, "selftest%.lua:%d+: harness: DCS%.getPuase is not modelled$",
  "an unmodelled name raises where it is read, naming its path")
