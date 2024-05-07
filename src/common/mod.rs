use std::time::{Duration, SystemTime, UNIX_EPOCH};
/// Retrieve the current unix time in nanoseconds
pub fn get_unix_time_millis() -> u128 {
    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_else(|_| {
        println!("Failed computing UNIX timestamp during admin login!");
        Duration::from_secs(0)
    });
    return duration.as_millis();
}
/// Retrieve the current unix time in nanoseconds
pub fn get_unix_time_seconds() -> u64 {
    let now = SystemTime::now();
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_else(|_| {
        println!("Failed computing UNIX timestamp during admin login!");
        Duration::from_secs(0)
    });
    return duration.as_secs();
}
use thiserror::Error;


#[derive(Error,Debug)]
pub enum GetError {
    #[error("Not found")]
    NotFound,
    #[error("Could not communicate with database")]
    Database,
    #[error("Could not deserialize binary data")]
    Deserialize,

}

#[derive(Error,Debug)]
pub enum SetError {
    #[error("Could not communicate with database")]
    Database,
    #[error("Could not deserialize binary data")]
    Serialize,
}


#[derive(Error,Debug)]
pub enum DatabaseError {
    #[error("No matches found")]
    NotFound,
    #[error("Could not get from database")]
    Get,
    #[error("Could not get from database")]
    Set,
}

#[derive(Error,Debug)]
pub enum DeleteError {
    #[error("No matches found")]
    NotFound,
    #[error("Could not delete from database")]
    NoDelete,    
}



