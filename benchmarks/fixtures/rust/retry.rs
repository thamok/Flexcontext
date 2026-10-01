pub struct RetryPolicy { pub max_attempts: u32, pub base_ms: u64 }
/// Exponential backoff between failed attempts.
pub fn retry_delay(policy: &RetryPolicy, attempt: u32) -> u64 { policy.base_ms.saturating_mul(2u64.saturating_pow(attempt)) }
/// Stop retrying when the maximum attempt count is reached.
pub fn should_retry(policy: &RetryPolicy, attempt: u32) -> bool { attempt < policy.max_attempts }
/// Schedule another attempt only if retries remain.
pub fn schedule_retry(policy: &RetryPolicy, attempt: u32) -> Option<u64> { if should_retry(policy, attempt) { Some(retry_delay(policy, attempt)) } else { None } }
pub fn retry_button_text() -> &'static str { "Retry" }
