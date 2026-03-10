use alloy::providers::{Provider, ProviderBuilder};

use crate::gateway::{get_unix_time_seconds, PaymentGateway};
use crate::invoice::Invoice;
use crate::web3::result::Result;
use crate::web3::transfers::native_transfers::transfer_native_to_treasury;

use super::InvoicePoller;

impl<P: Provider + Sync> InvoicePoller<P> {
    /// Checks if enough native currency has been received to cover the invoice.
    async fn check_invoice(&self, invoice: &Invoice) -> Result<bool> {
        let balance = self.provider.get_balance(invoice.to).await?;
        Ok(balance >= invoice.amount)
    }

    /// Runs the polling loop, checking all pending invoices on each iteration.
    pub(crate) async fn poll(&self) {
        loop {
            tracing::info!(
                "Pending invoices: {:?}",
                self.gateway.invoices.read().await.len()
            );
            match self.gateway.get_all_invoices().await {
                Ok(all) => {
                    for (key, mut invoice) in all {
                        if get_unix_time_seconds() > invoice.expires {
                            self.gateway.invoices.write().await.remove(&key);
                            continue;
                        }

                        if invoice.paid_at_timestamp > 0 {
                            continue;
                        }

                        let is_paid =
                            self.check_invoice(&invoice)
                                .await
                                .unwrap_or_else(|error| {
                                    tracing::error!("Failed to check balance: {}", error);
                                    false
                                });

                        if is_paid {
                            tracing::info!("Starting transfer to treasury");
                            match transfer_native_to_treasury(
                                self.gateway.clone(),
                                &invoice,
                            )
                            .await
                            {
                                Ok(receipt) => {
                                    invoice.hash = Some(receipt);
                                }
                                Err(error) => {
                                    tracing::error!(
                                        "Could not transfer paid invoice to treasury: {}",
                                        error
                                    );
                                }
                            }
                            invoice.paid_at_timestamp = get_unix_time_seconds();

                            self.gateway
                                .invoices
                                .write()
                                .await
                                .insert(key.clone(), invoice.clone());

                            if let Err(error) =
                                self.gateway.config.sender.send((key, invoice))
                            {
                                tracing::error!("Failed sending data: {}", error);
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(
                            self.gateway.config.poller_delay_seconds,
                        ))
                        .await;
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Could not get all invoices, did not callback: {}",
                        error
                    );
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(
                self.gateway.config.poller_delay_seconds,
            ))
            .await;
        }
    }
}

/// Creates an `InvoicePoller` and starts the polling loop.
pub async fn poll_payments(gateway: PaymentGateway) {
    tracing::info!("Starting polling payments");
    let provider = ProviderBuilder::new()
        .connect_http(gateway.config.rpc_url.parse().expect("Invalid RPC URL"));
    let poller = InvoicePoller::new(provider, gateway);
    poller.poll().await;
}
