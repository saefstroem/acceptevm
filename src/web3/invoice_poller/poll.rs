use alloy::providers::{Provider, ProviderBuilder};

use crate::gateway::{get_unix_time_seconds, PaymentGateway};
use crate::invoice::Invoice;
use crate::web3::result::Result;
use crate::web3::transfers::native_transfers::{confirm_treasury_transfer, send_native_to_treasury};

use super::InvoicePoller;

impl InvoicePoller {
    /// Checks if enough native currency has been received to cover the invoice.
    async fn check_invoice(&self, provider: &impl Provider, invoice: &Invoice) -> Result<bool> {
        let balance = provider.get_balance(invoice.to).await?;
        Ok(balance >= invoice.amount)
    }

    /// Runs the polling loop. Each cycle picks the next RPC URL via round-robin.
    pub(crate) async fn poll(&self) {
        loop {
            let rpc_url = self.gateway.next_rpc_url();
            let url = match rpc_url.parse() {
                Ok(url) => url,
                Err(error) => {
                    tracing::error!("Invalid RPC URL '{}': {}", rpc_url, error);
                    tokio::time::sleep(std::time::Duration::from_secs(
                        self.gateway.config.poller_delay_seconds,
                    ))
                    .await;
                    continue;
                }
            };
            let provider = ProviderBuilder::new().connect_http(url);

            tracing::info!(
                "Pending invoices: {:?}",
                self.gateway.invoices.read().await.len()
            );
            match self.gateway.get_all_invoices().await {
                Ok(all) => {
                    for (key, mut invoice) in all {
                        if let Some(ref tx_hash) = invoice.hash {
                            match confirm_treasury_transfer(&self.gateway, tx_hash).await {
                                Ok(true) => {
                                    tracing::info!("Treasury transfer confirmed: {}", tx_hash);
                                    invoice.paid_at_timestamp = get_unix_time_seconds();

                                    self.gateway.invoices.write().await.remove(&key);

                                    if let Err(error) =
                                        self.gateway.config.sender.send((key, invoice))
                                    {
                                        tracing::error!("Failed sending data: {}", error);
                                    }
                                }
                                Ok(false) => {
                                    tracing::info!(
                                        "Tx {} not yet confirmed, retrying with bumped fees",
                                        tx_hash
                                    );
                                    match send_native_to_treasury(&self.gateway, &invoice).await {
                                        Ok((new_hash, nonce)) => {
                                            invoice.hash = Some(new_hash);
                                            invoice.nonce = Some(nonce);
                                            self.gateway
                                                .invoices
                                                .write()
                                                .await
                                                .insert(key.clone(), invoice);
                                        }
                                        Err(error) => {
                                            tracing::error!(
                                                "Failed to send replacement tx: {}",
                                                error
                                            );
                                        }
                                    }
                                }
                                Err(error) => {
                                    tracing::error!(
                                        "Error checking treasury transfer: {}",
                                        error
                                    );
                                }
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(
                                self.gateway.config.poller_delay_seconds,
                            ))
                            .await;
                            continue;
                        }

                        let is_paid = match self.check_invoice(&provider, &invoice).await {
                            Ok(paid) => paid,
                            Err(error) => {
                                tracing::error!("Failed to check balance: {}", error);
                                continue;
                            }
                        };

                        // Only remove expired invoices that have not been paid
                        if !is_paid && get_unix_time_seconds() > invoice.expires {
                            self.gateway.invoices.write().await.remove(&key);
                            continue;
                        }

                        // Paid — send initial treasury transfer
                        if is_paid {
                            tracing::info!("Invoice paid, sending to treasury");
                            match send_native_to_treasury(&self.gateway, &invoice).await {
                                Ok((hash, nonce)) => {
                                    invoice.hash = Some(hash);
                                    invoice.nonce = Some(nonce);
                                    self.gateway
                                        .invoices
                                        .write()
                                        .await
                                        .insert(key.clone(), invoice);
                                }
                                Err(error) => {
                                    tracing::error!(
                                        "Failed to send treasury transfer: {}",
                                        error
                                    );
                                }
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
    let poller = InvoicePoller::new(gateway);
    poller.poll().await;
}
