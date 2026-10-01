export interface Session { userId: string; expiresAt: number }
/** An expired login session must not authorize a request. */
export function sessionExpired(session: Session, now: number): boolean { return now >= session.expiresAt; }
/** Read the authenticated user only from an active session. */
export function sessionUser(session: Session, now: number): string | null { return sessionExpired(session, now) ? null : session.userId; }
/** Refresh the session expiry while preserving the user. */
export function refreshSession(session: Session, now: number): Session { return { ...session, expiresAt: now + 3600 }; }
export function sessionPanelTitle(): string { return "Login sessions"; }
