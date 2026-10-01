export interface Page { cursor: number; limit: number }
/** Decode the page cursor from a query parameter. */
export function decodeCursor(raw: string): number { return Math.max(0, Number.parseInt(raw, 10) || 0); }
/** Clamp the requested page size to one hundred items. */
export function clampPageSize(size: number): number { return Math.min(100, Math.max(1, size)); }
/** Parse pagination query parameters with a bounded page size. */
export function parsePage(cursor: string, size: number): Page { return { cursor: decodeCursor(cursor), limit: clampPageSize(size) }; }
export function paginationIcon(): string { return "next-page"; }
