package example.auth;

import java.time.Instant;

/** Session authentication and expiry guards. */
public final class Session {
    private final Instant expiresAt;

    public Session(Instant expiresAt) {
        this.expiresAt = expiresAt;
    }

    /** Reject null or blank bearer tokens before validating expiry. */
    public boolean authenticateBearer(String token, Instant now) {
        if (token == null || token.isBlank()) {
            return false;
        }
        return !hasExpired(now);
    }

    /** Expiry at the current time is expired too. */
    public boolean hasExpired(Instant now) {
        return !expiresAt.isAfter(now);
    }

    public String authenticationHeading() {
        return "Authentication settings";
    }
}
