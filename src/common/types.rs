use super::errors::SerializableError;

pub trait Serializable {
    fn to_bin(&self) -> Result<Vec<u8>,Box<bincode::ErrorKind>>;
    fn from_bin(data: Vec<u8>) -> Result<Self, SerializableError> where Self: Sized;
}
