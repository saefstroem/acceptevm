# acceptevm - Accept EVM Payments in Your Application

A lightweight Rust library for accepting native cryptocurrency payments on any EVM-compatible network. Built on top of [alloy-rs](https://github.com/alloy-rs/alloy) for robust Ethereum interaction. The name was greatly inspired by [acceptxmr](https://github.com/busyboredom/acceptxmr), a similar library but for Monero.

## Features

* Accept native token payments (ETH, BNB, MATIC, etc.) on any EVM network.
* Lightweight and easy to integrate.
* Automatic fund sweeping to your treasury address.
* Configurable polling interval and confirmation requirements.
* Paid invoices delivered via tokio mpsc channel for flexible handling.
* Round-robin RPC URL balancing across multiple providers.

## Why acceptevm?

As Web3 developers and payment operators, we often need to accept cryptocurrency payments in our applications. However, setting up a payment flow that generates unique deposit addresses, monitors balances, and sweeps funds to a treasury is non-trivial. acceptevm handles all of this out of the box, so you can focus on your application logic.

Currently, only native token payments are supported. ERC20 token support is planned for a future release.

## Installation

To use acceptevm in your project, add the following to your `Cargo.toml` file:

```toml
[dependencies]
acceptevm = "0.0.4"
```

## Usage

Here is a simple example of how to use acceptevm to accept payments:

```rust
use acceptevm::gateway::{
    Address, PaymentGateway, PaymentGatewayConfiguration, Wei,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a channel to receive paid invoices
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();

    // Configure the payment gateway
    let gateway = PaymentGateway::new(PaymentGatewayConfiguration {
        rpc_urls: vec![
            "https://bsc-dataseed1.binance.org/".to_string(),
            "https://bsc-dataseed2.binance.org/".to_string(),
        ],
        treasury_address: "0xdac17f958d2ee523a2206206994597c13d831ec7"
            .parse::<Address>()?,
        min_confirmations: 10,
        sender,
        poller_delay_seconds: 10,
    })?;

    // Create a new invoice
    let (invoice_id, invoice) = gateway
        .new_invoice(
            Wei::from(100),
            b"Invoice details".to_vec(),
            3600,
        )
        .await?;

    // Start polling for payments
    gateway.poll_payments().await;

    // Receive paid invoices
    while let Some((id, paid_invoice)) = receiver.recv().await {
        println!("Invoice {} paid!", id);
        if let Some(hash) = &paid_invoice.hash {
            println!("Transaction hash: {}", hash);
        }
    }

    Ok(())
}
```

You can also loop through the receiver to continuously process paid invoices in real-time:

```rust,ignore
while let Some((id, paid_invoice)) = receiver.recv().await {
    println!("Invoice {} paid!", id);
}
```

## How Does It Work?

The **PaymentGateway** serves as the core component of the library, designed to be instantiated for each EVM network. For each invoice, a unique wallet is generated. The gateway periodically checks whether the required amount has been deposited to the invoice's address. Once payment is detected and confirmed, the funds are automatically swept to the configured treasury address.

Upon receipt of payment for an invoice, the system sends the relevant invoice data through a tokio mpsc channel. This provides you with the flexibility to implement any desired actions in response, such as crediting a user's account or executing other specified tasks.

**Important:** Due to the uncertainty of blockchain transactions, the treasury transfer could fail. Always check if the `hash` field is present in the paid invoice. If the hash is present, the funds were successfully transferred to the treasury. If not, the invoice's `wallet` field contains the private key bytes that can be used to recover the funds via `alloy::signers::local::PrivateKeySigner::from_bytes()` or other means.

## License

This library is licensed under the MIT license. See the [LICENSE](LICENSE) file for more details.
