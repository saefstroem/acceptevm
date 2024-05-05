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



