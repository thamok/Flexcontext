export interface OrderView { id: string; totalCents: number }
/** Format order price for the checkout display. */
export function formatOrderPrice(order: OrderView): string { return (order.totalCents / 100).toFixed(2); }
/** Render the checkout order summary. */
export function renderOrderSummary(order: OrderView): string { return `Order ${order.id}: ${formatOrderPrice(order)}`; }
/** Build a link to the order detail route. */
export function orderDetailUrl(order: OrderView): string { return `/orders/${encodeURIComponent(order.id)}`; }
export function orderTotalMetric(): string { return "order_total_count"; }
