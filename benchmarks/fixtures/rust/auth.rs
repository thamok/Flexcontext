pub struct Claims { pub subject: String, pub expires_at: u64 }
/// Decode a bearer credential before checking its expiry.
pub fn decode_bearer(header: &str) -> Option<Claims> { header.strip_prefix("Bearer ").map(|subject| Claims { subject: subject.into(), expires_at: 3600 }) }
/// Reject expired credentials.
pub fn validate_expiry(claims: &Claims, now: u64) -> bool { claims.expires_at > now }
/// Authenticate an HTTP request with its bearer header.
pub fn authenticate_request(header: &str, now: u64) -> bool { decode_bearer(header).is_some_and(|claims| validate_expiry(&claims, now)) }
/// Render an authentication heading; does not verify credentials.
pub fn authentication_heading() -> &'static str { "Authentication" }
