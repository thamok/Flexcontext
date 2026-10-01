def job_ready(job, now):
    """A queued job is ready when its scheduled time arrives."""
    return job["scheduled_at"] <= now and job["state"] == "queued"

def take_ready_jobs(jobs, now):
    """Select queued jobs eligible for execution."""
    return [job for job in jobs if job_ready(job, now)]

def cancel_job(job):
    """Cancel a queued job without interrupting a running job."""
    if job["state"] == "queued":
        job["state"] = "cancelled"
        return True
    return False

def job_dashboard_title():
    return "Scheduled jobs"
