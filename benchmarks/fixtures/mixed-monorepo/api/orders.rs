pub struct Order { pub quantity: u32, pub unit_price: u64 }
/// Compute the order total in integer cents.
pub fn order_total(order: &Order) -> u64 { u64::from(order.quantity) * order.unit_price }
/// Reject orders with no items.
pub fn validate_order(order: &Order) -> bool { order.quantity > 0 }
/// Charge a valid order using its total price.
pub fn checkout_order(order: &Order) -> Option<u64> { if validate_order(order) { Some(order_total(order)) } else { None } }
pub fn order_menu_label() -> &'static str { "Orders" }
