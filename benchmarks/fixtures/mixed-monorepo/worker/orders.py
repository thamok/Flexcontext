def export_order_row(order):
    """Encode an order for the warehouse export."""
    return {"id": order["id"], "total_cents": order["total_cents"]}

def export_orders(orders):
    """Batch export orders to warehouse rows."""
    return [export_order_row(order) for order in orders]

def pending_exports(orders):
    """Find orders not yet exported to the warehouse."""
    return [order for order in orders if not order.get("exported")]

def order_export_title():
    return "Order exports"
