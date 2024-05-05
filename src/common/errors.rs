use thiserror::Error;

#[derive(Error,Debug,PartialEq)]
pub enum DeserializeError {
    #[error("Invalid body")]
    InvalidBody, 
}

#[derive(Error,Debug)]
pub enum SerializableError {
    #[error("Could not deserialize binary")]
    Deserialize,
 
}



