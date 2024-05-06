use thiserror::Error;

#[derive(Error,Debug)]
pub enum SerializableError {
    #[error("Could not deserialize binary")]
    Deserialize,
 
}







