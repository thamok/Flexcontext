export interface Upload { name: string; bytes: Uint8Array }
/** Reject oversized uploads before storing their bytes. */
export function validateUploadSize(upload: Upload, max: number): boolean { return upload.bytes.byteLength <= max; }
/** Remove directory components from an upload filename. */
export function safeUploadName(name: string): string { return name.split(/[\\/]/).pop() || "upload"; }
/** Prepare a safe filename only for an upload within the byte limit. */
export function prepareUpload(upload: Upload, max: number): string | null { return validateUploadSize(upload, max) ? safeUploadName(upload.name) : null; }
export function uploadProgressLabel(): string { return "Uploading"; }
