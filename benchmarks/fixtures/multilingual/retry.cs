using System;
namespace Requests {
    public class RetryPolicy {
        public int DelayMilliseconds(int attempt) {
            return Math.Min(30000, 100 * (1 << attempt));
        }
        public int RetryRequest(int status, int attempt) {
            if (status != 429 && status < 500) { return -1; }
            if (attempt >= 5) { return -1; }
            return DelayMilliseconds(attempt);
        }
        public string RetryHeading() { return "Request retry status attempt delay milliseconds"; }
    }
}
