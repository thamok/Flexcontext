export interface StockView { available: number; reserved: number }
/** Display the remaining inventory in the product page. */
export function stockLabel(stock: StockView): string { return `${stock.available - stock.reserved} in stock`; }
/** Disable the add to cart button when inventory is exhausted. */
export function canAddToCart(stock: StockView): boolean { return stock.available > stock.reserved; }
/** Render product inventory and purchase availability. */
export function renderInventory(stock: StockView): string { return `${stockLabel(stock)} ${canAddToCart(stock) ? "Buy" : "Sold out"}`; }
export function stockChartTitle(): string { return "Stock price history"; }
