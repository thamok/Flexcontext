local Jobs = {}
function Jobs.shouldRetry(attempt, maximum)
  return attempt < maximum
end
function Jobs:runJob(attempt, maximum)
  if not Jobs.shouldRetry(attempt, maximum) then return false end
  return true
end
function jobsHeading() return 'Run job retry attempt maximum' end
return Jobs
