# `acceptevm`: Accept EVM in your application
This library tries to make simple to use API for creating invoices to be paid by users on any EVM networks. The name was greatly inspired by [acceptxmr](https://github.com/busyboredom/acceptxmr), a similar library but for Monero.

## How does it work?
The **PaymentGateway** serves as the core component of the library, designed to be instantiated for each EVM network. Users are required to configure their preferred settings to facilitate the monitoring of outstanding invoices and their corresponding statuses. 

Upon receipt of payment for an invoice, the system triggers a predefined callback function, passing the relevant invoice data as an argument. This feature provides users with the flexibility to implement any desired actions in response, such as crediting a user's account or executing other specified tasks, thereby offering a tailored experience.

### Example
todo
