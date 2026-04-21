use alloy::providers::{Provider, ProviderBuilder};

use crate::gateway::{get_unix_time_seconds, PaymentGateway};
use crate::invoice::Invoice;
use crate::web3::result::Result;
use crate::web3::transfers::native_transfers::{
    confirm_treasury_transfer, send_native_to_treasury,
};

use super::InvoicePoller;

impl InvoicePoller {
    async fn check_invoice(&self, provider: &impl Provider, invoice: &Invoice) -> Result<bool> {
        Ok(provider.get_balance(invoice.to).await? >= invoice.amount)
    }

    pub(crate) async fn poll(&self) {
        loop {
            self.poll_cycle().await;
            self.delay().await;
        }
    }

    async fn poll_cycle(&self) {
        let rpc_url = self.gateway.next_rpc_url();
        let url = match rpc_url.parse() {
            Ok(url) => url,
            Err(e) => {
                tracing::error!("Invalid RPC URL '{rpc_url}': {e}");
                return;
            }
        };
        let provider = ProviderBuilder::new().connect_http(url);

        tracing::info!(
            "Pending invoices: {}",
            self.gateway.invoices.read().await.len()
        );

        let all = match self.gateway.get_all_invoices().await {
            Ok(all) => all,
            Err(e) => {
                tracing::error!("Could not get all invoices: {e}");
                return;
            }
        };

        for (key, mut invoice) in all {
            self.process_invoice(&provider, &key, &mut invoice).await;
            self.delay().await;
        }
    }

    async fn process_invoice(&self, provider: &impl Provider, key: &str, invoice: &mut Invoice) {
        if invoice.amount.is_zero() {
            tracing::info!("No charge for invoice, confirming");
            invoice.paid_at_timestamp = get_unix_time_seconds();
            self.send_confirmed_invoice(key, invoice.clone()).await;
            return;
        }

        if invoice.hash.is_some() {
            self.handle_pending_tx(key, invoice).await;
            return;
        }

        let is_paid = match self.check_invoice(provider, invoice).await {
            Ok(paid) => paid,
            Err(e) => {
                tracing::error!("Failed to check balance: {e}");
                return;
            }
        };

        if !is_paid {
            if get_unix_time_seconds() > invoice.expires {
                self.gateway.invoices.write().await.remove(key);
            }
            return;
        }

        tracing::info!("Invoice paid, sending to treasury");
        self.send_to_treasury(key, invoice).await;
    }

    async fn handle_pending_tx(&self, key: &str, invoice: &mut Invoice) {
        let confirmed = match invoice.hash.as_deref() {
            Some(tx_hash) => confirm_treasury_transfer(&self.gateway, tx_hash).await,
            None => return,
        };

        match confirmed {
            Ok(true) => {
                tracing::info!(
                    "Treasury transfer confirmed: {}",
                    invoice.hash.as_deref().unwrap_or("unknown")
                );
                invoice.paid_at_timestamp = get_unix_time_seconds();
                self.send_confirmed_invoice(key, invoice.clone()).await;
            }
            Ok(false) => {
                tracing::info!(
                    "Tx {} not yet confirmed, retrying with bumped fees",
                    invoice.hash.as_deref().unwrap_or("unknown")
                );
                self.send_to_treasury(key, invoice).await;
            }
            Err(e) => tracing::error!("Error checking treasury transfer: {e}"),
        }
    }

    async fn send_to_treasury(&self, key: &str, invoice: &mut Invoice) {
        match send_native_to_treasury(&self.gateway, invoice).await {
            Ok((hash, nonce)) => {
                invoice.hash = Some(hash);
                invoice.nonce = Some(nonce);
                self.gateway
                    .invoices
                    .write()
                    .await
                    .insert(key.to_string(), invoice.clone());
            }
            Err(e) => tracing::error!("Failed to send treasury transfer: {e}"),
        }
    }

    async fn send_confirmed_invoice(&self, key: &str, invoice: Invoice) {
        self.gateway.invoices.write().await.remove(key);
        if let Err(e) = self.gateway.config.sender.send((key.to_string(), invoice)) {
            tracing::error!("Failed sending data: {e}");
        }
    }

    async fn delay(&self) {
        tokio::time::sleep(std::time::Duration::from_secs(
            self.gateway.config.poller_delay_seconds,
        ))
        .await;
    }
}

pub async fn poll_payments(gateway: PaymentGateway) {
    tracing::info!("Starting polling payments");
    InvoicePoller::new(gateway).poll().await;
}
