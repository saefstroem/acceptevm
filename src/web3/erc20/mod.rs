
use std::sync::Arc;

use ethers::{abi::Abi, contract::{Contract, ContractError}, providers::{Http, Provider}, types::{Address, U256}};

#[derive(Clone)]
pub struct ERC20Token {
    pub contract: Contract<Provider<Http>>,
}

impl ERC20Token {
    /// Creates a new instance of an ERC20 token. This is just a wrapper
    /// function to simplify the interactions with contracts.
    pub fn new(provider: Provider<Http>, token_address: Address) -> ERC20Token {
        let abi:Abi=serde_json::from_str(include_str!("IERC20.json")).unwrap();
        let contract = Contract::new(token_address, abi, Arc::new(provider));
        ERC20Token { contract }
    }

    /// Retrieves the token balance of a specified address
    pub async fn get_balance(&self, address: Address) -> Result<U256, ContractError<Provider<Http>>> {
        let balance=self.contract.method::<Address,U256>("balanceOf", address).unwrap()
        .call().await?;
        Ok(balance)
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use ethers::{providers::Provider, types::{Address, U256}};

    use crate::web3::erc20::ERC20Token;
    #[tokio::test]
    async fn valid_balance() {
        let provider = Provider::try_from("https://bsc-dataseed1.binance.org/").unwrap();

        let token = ERC20Token::new(
            provider,
            "0x2170ed0880ac9a755fd29b2688956bd959f933f8".parse::<Address>().unwrap(),
        );
        let balance = token
            .get_balance("0xC882b111A75C0c657fC507C04FbFcD2cC984F071".parse::<Address>().unwrap())
            .await
            .unwrap();
        println!("Balance check: {}", balance);
        assert!(balance.ge(&U256::from_str("0").unwrap()));
    }
}
