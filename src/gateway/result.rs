use super::error::GatewayError;

pub type Result<T> = std::result::Result<T, GatewayError>;
