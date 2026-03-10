mod poll;

use alloy::providers::Provider;

use crate::gateway::PaymentGateway;

pub use poll::poll_payments;

/// Periodically checks invoices for incoming payments using a read-only provider.
pub(crate) struct InvoicePoller<P> {
    pub(crate) provider: P,
    pub(crate) gateway: PaymentGateway,
}

impl<P: Provider + Sync> InvoicePoller<P> {
    pub(crate) fn new(provider: P, gateway: PaymentGateway) -> Self {
        Self { provider, gateway }
    }
}
