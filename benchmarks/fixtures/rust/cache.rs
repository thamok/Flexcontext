pub struct CacheEntry { pub value: String, pub expires_at: u64 }
/// A cached value is fresh until its time to live expires.
pub fn is_fresh(entry: &CacheEntry, now: u64) -> bool { now < entry.expires_at }
/// Return a hit only while the cached value is fresh.
pub fn lookup_cache(entry: &CacheEntry, now: u64) -> Option<&str> { if is_fresh(entry, now) { Some(&entry.value) } else { None } }
/// Remove stale entries from the cache.
pub fn evict_expired(entries: &mut Vec<CacheEntry>, now: u64) { entries.retain(|entry| is_fresh(entry, now)); }
pub fn cache_directory_label() -> &'static str { "cache" }
