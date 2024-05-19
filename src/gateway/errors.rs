use thiserror::Error;

#[derive(Error, Debug)]
pub enum GatewayError {
    #[error("No matches found")]
    NotFound,
    #[error("Could not get from database")]
    Get,
    #[error("Could not set to database")]
    Set,
    #[error("Could not communicate with database")]
    Communicate,
    #[error("Could not deserialize binary data")]
    Deserialize,
    #[error("Could not serialize binary data")]
    Serialize,
    #[error("Could not delete from database")]
    NoDelete,
}
