use thiserror::Error;

#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("No matches found")]
    NotFound,
    #[error("No RPC URLs provided")]
    NoRpcUrls
}
