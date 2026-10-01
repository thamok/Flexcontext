export interface WebhookEvent { id: string; payload: string }
/** Reject duplicate webhook deliveries using the event ID. */
export function alreadyDelivered(event: WebhookEvent, seen: Set<string>): boolean { return seen.has(event.id); }
/** Record successful webhook delivery for idempotency. */
export function markDelivered(event: WebhookEvent, seen: Set<string>): void { seen.add(event.id); }
/** Process each webhook event at most once. */
export function acceptWebhook(event: WebhookEvent, seen: Set<string>): boolean { if (alreadyDelivered(event, seen)) return false; markDelivered(event, seen); return true; }
export function webhookSettingsTitle(): string { return "Webhook settings"; }
