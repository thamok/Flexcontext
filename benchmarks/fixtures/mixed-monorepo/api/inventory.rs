pub struct Stock { pub available: u32, pub reserved: u32 }
/// Quantity still available after existing reservations.
pub fn available_stock(stock: &Stock) -> u32 { stock.available.saturating_sub(stock.reserved) }
/// Reserve stock only when enough inventory remains.
pub fn reserve_stock(stock: &mut Stock, quantity: u32) -> bool { if available_stock(stock) < quantity { return false; } stock.reserved += quantity; true }
/// Release stock reserved by a cancelled order.
pub fn release_stock(stock: &mut Stock, quantity: u32) { stock.reserved = stock.reserved.saturating_sub(quantity); }
pub fn inventory_page_title() -> &'static str { "Inventory" }
