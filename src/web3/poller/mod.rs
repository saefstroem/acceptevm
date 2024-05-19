
use ethers::contract::ContractError;
use ethers::providers::{Http, Provider};
use crate::gateway::Reflector::Sender;
use crate::gateway::{get_unix_time_seconds, PaymentGateway};
use crate::invoice::Invoice;

use super::erc20::ERC20Token;
use super::transfers::gas_transfers::transfer_gas_to_treasury;
use super::{get_native_balance, TransferError};

/// Checks if a specific token of a specific amount has been received
/// at a certain address.
async fn check_if_token_received(
    token: ERC20Token,
    invoice: &Invoice,
) -> Result<bool, ContractError<Provider<Http>>> {
    let balance_of_recipient = token.get_balance(invoice.to).await?;
    if balance_of_recipient.ge(&invoice.amount) {
        return Ok(true);
    }
    Ok(false)
}

/// Used to check if the invoice recipient has received enough money to cover the invoice
async fn check_if_native_received(
    provider: Provider<Http>,
    invoice: &Invoice,
) -> Result<bool, TransferError> {
    let balance_of_recipient = get_native_balance(&provider, &invoice.to).await?;
    if balance_of_recipient.ge(&invoice.amount) {
        return Ok(true);
    }
    Ok(false)
}

/// A function that branches control flow depending on the invoice shall
/// be paid by an ERC20-compatible token or the native gas token on the network
async fn check_and_process(provider: Provider<Http>, invoice: &Invoice) -> bool {
    match &invoice.token_address {
        Some(address) => {
            let token = ERC20Token::new(provider, *address);
            check_if_token_received(token, invoice).await.unwrap_or_else(|error| {
                log::error!("Failed to check balance: {}", error);
                false
            })
        }
        None => check_if_native_received(provider, invoice).await.unwrap_or_else(|error| {
            log::error!("Failed to check balance: {}", error);
            false
        }),
    }
}


async fn transfer_to_treasury(
    gateway: PaymentGateway,
    invoice: &Invoice,
) -> Result<String, TransferError> {
    transfer_gas_to_treasury(gateway, invoice).await
}

/// Periodically checks if invoices are paid in accordance
/// to the specified polling interval.
pub async fn poll_payments(gateway: PaymentGateway) {
    log::info!("Starting polling payments");
    loop {
        log::info!("Pending invoices: {:?}", gateway.invoices.len());
        match gateway.get_all_invoices().await {
            Ok(all) => {
                // Loop through all invoices
                for (key, mut invoice) in all {
                    // If the current time is greater than expiry
                    if get_unix_time_seconds() > invoice.expires {
                        // Delete the invoice and continue with the next iteration
                        gateway.invoices.remove(&key);
                        continue;
                    }
                    // Check if the invoice was paid
                    let check_result =
                        check_and_process(gateway.config.provider.clone(), &invoice).await;

                    if check_result {
                        log::info!("Starting transfer to treasury");
                        // Attempt transfer to treasury
                        match transfer_to_treasury(gateway.clone(), &invoice).await {
                            Ok(receipt) => {
                                invoice.hash = Some(receipt);
                            }
                            Err(error) => {
                                log::error!(
                                    "Could not transfer paid invoice to treasury: {}",
                                    error
                                );
                            }
                        }

                        // If the transfer_to_treasury invoice was paid, delete it, stand in queue for the
                        // lock to the callback function.
                        gateway.invoices.remove(&key);
                        invoice.paid_at_timestamp = get_unix_time_seconds();
                        match gateway.config.reflector {
                            Sender(ref sender) => {
                                // Attempt to send the PriceData through the channel.
                                if let Err(error) = sender.send((key,invoice)).await {
                                    log::error!("Failed sending data: {}", error);
                                }
                            }
                        }
                    }
                    // To prevent rate limitations on certain Web3 RPC's we sleep here for the specified amount.
                    tokio::time::sleep(std::time::Duration::from_secs(
                        gateway.config.poller_delay_seconds,
                    ))
                    .await;
                }
            }
            Err(error) => {
                log::error!("Could not get all invoices, did not callback: {}", error);
            }
        }
        // To prevent busy idling we sleep here too.
        tokio::time::sleep(std::time::Duration::from_secs(
            gateway.config.poller_delay_seconds,
        ))
        .await;
    }
}

#[cfg(test)]
mod tests {

    use ethers::{providers::Provider, types::{Address, U256}};

    use crate::web3::get_native_balance;


    #[tokio::test]
    async fn valid_balance() {
        let provider=Provider::try_from("https://bsc-dataseed1.binance.org/").unwrap();
        let balance = get_native_balance(
            &provider,
            &"0x2170ed0880ac9a755fd29b2688956bd959f933f8".parse::<Address>().unwrap(),
        )
        .await
        .unwrap();
        println!("Balance check: {}", balance);
        assert!(balance.ge(&U256::from(0)));
    }
}
