mod abi;
use web3::{
    contract::{Contract, Options},
    transports::Http,
    types::{Address, U256},
    Web3,
};

use std::str::FromStr;

use self::abi::ERC20_ABI;
#[derive(Clone)]
pub struct ERC20Token {
    contract: Contract<Http>,
}

impl ERC20Token {
    /// Creates a new instance of an ERC20 token. This is just a wrapper
    /// function to simplify the interactions with contracts.
    pub fn new(web3: Web3<Http>, token_address: String) -> ERC20Token {
        let contract = Contract::from_json(
            web3.eth(),
            token_address.parse().unwrap(),
            ERC20_ABI.as_bytes(),
        )
        .unwrap();
        ERC20Token { contract }
    }

    /// Retrieves the token balance of a specified address
    pub async fn get_balance(&self, address: String) -> Result<U256, web3::contract::Error> {
        self.contract
            .query(
                "balanceOf",
                Address::from_str(&address).unwrap(),
                None,
                Options::default(),
                None,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use web3::{transports::Http, types::U256, Web3};

    use crate::erc20::ERC20Token;

    #[tokio::test]
    async fn valid_balance() {
        let http = Http::new("https://bsc-dataseed1.binance.org/").unwrap();
        let web3 = Web3::new(http);
        let token = ERC20Token::new(
            web3,
            "0x2170ed0880ac9a755fd29b2688956bd959f933f8".to_string(),
        );
        let balance = token
            .get_balance("0xC882b111A75C0c657fC507C04FbFcD2cC984F071".to_string())
            .await
            .unwrap();
        println!("Balance check: {}", balance);
        assert!(balance.ge(&U256::zero()));
    }
}
