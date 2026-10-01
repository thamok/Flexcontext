def remaining_quota(limit, used):
    """Compute remaining request quota without going negative."""
    return max(0, limit - used)

def allow_request(limit, used):
    """Allow requests only while rate limit quota remains."""
    return remaining_quota(limit, used) > 0

def retry_after(reset_at, now):
    """Seconds until a throttled client may retry."""
    return max(0, reset_at - now)

def rate_limit_chart_label():
    return "Requests per minute"
