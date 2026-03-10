mod poll;

use crate::gateway::PaymentGateway;

pub use poll::poll_payments;

/// Periodically checks invoices for incoming payments.
/// Each poll cycle uses the next RPC URL via round-robin.
pub(crate) struct InvoicePoller {
    pub(crate) gateway: PaymentGateway,
}

impl InvoicePoller {
    pub(crate) fn new(gateway: PaymentGateway) -> Self {
        Self { gateway }
    }
}
