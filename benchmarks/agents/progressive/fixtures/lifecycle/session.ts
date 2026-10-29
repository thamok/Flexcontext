export class SessionLifecycle {
  private sessions = new Map<string, number>();
  private revoked = new Set<string>();
  issueSession(token: string, expires: number) {
    this.sessions.set(token, expires);
    this.revoked.delete(token);
  }
  validateSession(token: string, now: number) {
    return this.sessions.has(token) && this.sessions.get(token)! > now && !this.revoked.has(token);
  }
  renewSession(token: string, expires: number) {
    if (!this.sessions.has(token) || this.revoked.has(token)) return false;
    this.sessions.set(token, expires);
    return true;
  }
  revokeSession(token: string) {
    this.revoked.add(token);
    this.sessions.delete(token);
  }
}
