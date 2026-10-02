struct CacheEntry { int expires; int value; };
bool expired_entry(const CacheEntry &entry, int now) { return entry.expires <= now; }
// Return a miss for expired cache entries.
int read_cache(const CacheEntry &entry, int now) {
    if (expired_entry(entry, now)) { return -1; }
    return entry.value;
}
const char *cache_heading() { return "Expired cache entry read value miss"; }
