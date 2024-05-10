use crate::{
    audit::log_sync,
    common::get_unix_time_seconds,
    db::{delete, get_all},
    erc20::ERC20Token,
    gateway::PaymentGateway,
    transfers::{errors::TransferError, gas_transfers::transfer_gas_to_treasury},
    types::Invoice,
};

use alloy::{
    primitives::Uint,
    providers::{Provider, RootProvider},
    rpc::types::eth::TransactionReceipt,
    transports::http::Http,
};
use reqwest::Client;
use sled::Tree;

/// Checks if a specific token of a specific amount has been received
/// at a certain address.
async fn check_if_token_received(
    token: ERC20Token,
    invoice: Invoice,
) -> Result<bool, alloy::contract::Error> {
    let balance_of_recipient = token.get_balance(invoice.to).await?;
    if balance_of_recipient.ge(&invoice.amount) {
        return Ok(true);
    }
    Ok(false)
}

/// Retrieves the gas token balance of the specified address on the specified web3 instance
async fn get_native_balance(
    provider: RootProvider<Http<Client>>,
    address: String,
) -> Result<Uint<256, 4>, alloy::contract::Error> {
    Ok(provider
        .get_balance(address.parse().unwrap(), alloy::eips::BlockId::latest())
        .await?)
}

/// Used to check if the invoice recipient has received enough money to cover the invoice
async fn check_if_native_received(
    provider: RootProvider<Http<Client>>,
    invoice: Invoice,
) -> Result<bool, alloy::contract::Error> {
    let balance_of_recipient = get_native_balance(provider, invoice.to).await?;
    if balance_of_recipient.ge(&invoice.amount) {
        return Ok(true);
    }
    Ok(false)
}

/// A function that branches control flow depending on the invoice shall
/// be paid by an ERC20-compatible token or the native gas token on the network
async fn check_and_process(provider: RootProvider<Http<Client>>, invoice: Invoice) -> bool {
    match invoice.clone().method.token_address {
        Some(address) => {
            let token = ERC20Token::new(provider, address);
            match check_if_token_received(token, invoice).await {
                Ok(result) => result,
                Err(error) => {
                    log_sync(&format!("Failed to check balance: {}", error));
                    false
                }
            }
        }
        None => match check_if_native_received(provider, invoice).await {
            Ok(result) => result,
            Err(error) => {
                log_sync(&format!("Failed to check balance: {}", error));
                false
            }
        },
    }
}

async fn delete_invoice(tree: &Tree, key: String) {
    // Optimistically delete the old invoice.
    match delete(tree, &key).await {
        Ok(()) => {}
        Err(error) => {
            log_sync(&format!(
                "Could not remove invoice, did not callback: {}",
                error
            ));
        }
    }
}

async fn transfer_to_treasury(
    gateway: PaymentGateway,
    invoice: Invoice,
) -> Result<TransactionReceipt, TransferError> {
    transfer_gas_to_treasury(gateway, invoice).await
}

/// Periodically checks if invoices are paid in accordance
/// to the specified polling interval.
pub async fn poll_payments(gateway: PaymentGateway) {
    loop {
        match get_all::<Invoice>(&gateway.tree).await {
            Ok(all) => {
                // Loop through all invoices
                for mut entry in all {
                    // If the current time is greater than expiry
                    if get_unix_time_seconds() > entry.1.expires {
                        // Delete the invoice and continue with the next iteration
                        delete_invoice(&gateway.tree, entry.0).await;
                        continue;
                    }
                    // Check if the invoice was paid
                    let check_result =
                        check_and_process(gateway.config.provider.clone(), entry.clone().1).await;

                    if check_result {
                        // Attempt transfer to treasury
                        match transfer_to_treasury(gateway.clone(), entry.1.clone()).await {
                            Ok(receipt) => {
                                entry.1.receipt = Some(receipt);
                            }
                            Err(error) => {
                                log_sync(&format!(
                                    "Could not transfer paid invoice to treasury: {}",
                                    error
                                ));
                            }
                        }

                        // If the transfer_to_treasury invoice was paid, delete it, stand in queue for the
                        // lock to the callback function.
                        delete_invoice(&gateway.tree, entry.0).await;
                        let mut invoice = entry.1;
                        invoice.paid_at_timestamp = get_unix_time_seconds();
                        (gateway.config.callback)(invoice).await;// Execute callback function
                    }
                    // To prevent rate limitations on certain Web3 RPC's we sleep here for the specified amount.
                    tokio::time::sleep(std::time::Duration::from_millis(
                        gateway.config.invoice_delay_millis,
                    ))
                    .await;
                }
            }
            Err(error) => {
                log_sync(&format!(
                    "Could not get all invoices, did not callback: {}",
                    error
                ));
            }
        }
        // To prevent busy idling we sleep here too.
        tokio::time::sleep(std::time::Duration::from_millis(
            gateway.config.invoice_delay_millis,
        ))
        .await;
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use alloy::{primitives::U256, providers::ProviderBuilder};
    use reqwest::Url;

    use crate::poller::get_native_balance;

    #[tokio::test]
    async fn valid_balance() {
        let provider = ProviderBuilder::new()
            .on_http(Url::from_str("https://bsc-dataseed1.binance.org/").unwrap());
        let balance = get_native_balance(
            provider,
            "0x2170ed0880ac9a755fd29b2688956bd959f933f8".to_string(),
        )
        .await
        .unwrap();
        println!("Balance check: {}", balance);
        assert!(balance.ge(&U256::from_str("30000000000000000").unwrap()));
    }
}
