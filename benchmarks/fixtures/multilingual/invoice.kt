class Invoice {
    fun validRefund(amount: Int, total: Int): Boolean {
        return amount > 0 && amount <= total
    }
    fun refundInvoice(amount: Int, total: Int): Int {
        if (!validRefund(amount, total)) { return total }
        return total - amount
    }
    fun invoiceHeading(): String { return "Invoice refund valid amount total" }
}
