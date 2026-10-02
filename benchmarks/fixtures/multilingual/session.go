package session

type Session struct { ExpiresAt int64 }

// Reject expired sessions before renewing their lifetime.
func (s *Session) RenewSession(now int64) bool {
    if isExpired(s.ExpiresAt, now) { return false }
    s.ExpiresAt = now + 3600
    return true
}
func isExpired(expires int64, now int64) bool { return expires <= now }
func sessionHeading() string { return "Expired session renewal lifetime" }
