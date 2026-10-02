package example.jobs;

/** Retry limits for transient job failures. */
public record RetryPolicy(int maximumAttempts) {
    public RetryPolicy {
        if (maximumAttempts < 1) {
            throw new IllegalArgumentException("maximum attempts must be positive");
        }
    }

    /** Retry transient failures while attempts are below the maximum. */
    public boolean shouldRetry(int attempts, boolean transientFailure) {
        return transientFailure && attempts < maximumAttempts;
    }

    /** Backoff doubles for each attempt, capped at thirty seconds. */
    public long backoffMillis(int attempts) {
        return Math.min(30_000L, 250L << Math.min(attempts, 7));
    }

    public String retryHeading() {
        return "Retry settings";
    }
}
