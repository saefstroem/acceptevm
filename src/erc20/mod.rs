use alloy::{
    contract::Error, primitives::Uint, providers::RootProvider, sol, transports::http::Http,
};
use reqwest::Client;

use self::IERC20::IERC20Instance;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    IERC20,
    "src/abi/IERC20.json"
);

#[derive(Clone)]
pub struct ERC20Token {
    pub contract: IERC20Instance<Http<Client>, RootProvider<Http<Client>>>,
}

impl ERC20Token {
    /// Creates a new instance of an ERC20 token. This is just a wrapper
    /// function to simplify the interactions with contracts.
    pub fn new(provider: RootProvider<Http<Client>>, token_address: String) -> ERC20Token {
        let contract = IERC20::new(token_address.parse().unwrap(), provider);
        ERC20Token { contract }
    }

    /// Retrieves the token balance of a specified address
    pub async fn get_balance(&self, address: String) -> Result<Uint<256, 4>, Error> {
        let IERC20::balanceOfReturn { _0 } = self
            .contract
            .balanceOf(address.parse().unwrap())
            .call()
            .await?;
        Ok(_0)
    }
}

#[cfg(test)]
mod tests {

    use std::str::FromStr;

    use alloy::{primitives::U256, providers::ProviderBuilder};
    use reqwest::Url;

    use crate::erc20::ERC20Token;
    #[tokio::test]
    async fn valid_balance() {
        let provider = ProviderBuilder::new()
            .on_http(Url::from_str("https://bsc-dataseed1.binance.org/").unwrap());

        let token = ERC20Token::new(
            provider,
            "0x2170ed0880ac9a755fd29b2688956bd959f933f8".to_string(),
        );
        let balance = token
            .get_balance("0xC882b111A75C0c657fC507C04FbFcD2cC984F071".to_string())
            .await
            .unwrap();
        println!("Balance check: {}", balance);
        assert!(balance.ge(&U256::from_str("0").unwrap()));
    }
}
