export interface ParsedRequest { method: string; path: string }
/** Normalize an incoming HTTP method. */
export function normalizeMethod(method: string): string { return method.trim().toUpperCase(); }
/** Parse a request line into method and path. */
export function parseRequest(line: string): ParsedRequest { const [method, path] = line.split(" "); return { method: normalizeMethod(method), path }; }
/** Reject requests with unsupported HTTP methods. */
export function isAllowedMethod(method: string): boolean { return ["GET", "POST"].includes(normalizeMethod(method)); }
export function requestLogTitle(): string { return "Request log"; }
