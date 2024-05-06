pub mod errors;
use self::errors::{DatabaseError, DeleteError, GetError, SetError};
use crate::{audit::logger::log_sync, types::Serializable};
use sled::Tree;

async fn get_from_tree(db: &Tree, key: &str) -> Result<Vec<u8>, DatabaseError> {
    match db.get(key) {
        Ok(result) => match result {
            Some(value) => Ok(value.to_vec()),
            None => Err(DatabaseError::NotFound),
        },
        Err(_error) => Err(DatabaseError::Get),
    }
}

async fn get_all_from_tree(db: &Tree) -> Result<Vec<(Vec<u8>, Vec<u8>)>, DatabaseError> {
    let mut all = Vec::new();
    for el in db.iter() {
        match el {
            Ok(value) => {
                let el_bin_key = value.0.to_vec();
                let el_bin_value = value.1.to_vec();
                all.push((el_bin_key, el_bin_value));
            }
            Err(error) => {
                log_sync(&format!("Db Interaction Error: {}",error));
                return Err(DatabaseError::Get);
            }
        }
    }
    Ok(all)
}

async fn get_last_from_tree(db: &Tree) -> Result<(Vec<u8>, Vec<u8>), DatabaseError> {
    match db.last() {
        Ok(value) => match value {
            Some(tuple) => {
                let el_bin_key = tuple.0.to_vec();
                let el_bin_value = tuple.1.to_vec();
                return Ok((el_bin_key, el_bin_value));
            }
            None => return Err(DatabaseError::NotFound),
        },
        Err(error) => {
            log_sync(&format!("Db Interaction Error: {}",error));
            return Err(DatabaseError::Get);
        }
    }
}

pub async fn get_last<T: Serializable>(tree: &sled::Tree) -> Result<(String, T), GetError> {
    match get_last_from_tree(tree).await {
        Ok(binary_data) => {
            // Convert binary key to String
            let key = String::from_utf8(binary_data.0).map_err(|error| {
                log_sync(&format!("Db Interaction Error: {}", error));
                GetError::Deserialize
            })?;

            // Deserialize binary value to T
            let value = T::from_bin(binary_data.1).map_err(|error| {
                log_sync(&format!("Db Interaction Error: {}",error));
                GetError::Deserialize
            })?;
            Ok((key, value))
        }
        Err(error) => Err(match error {
            DatabaseError::NotFound => GetError::NotFound,
            _ => GetError::Database,
        }),
    }
}

pub async fn get_all<T: Serializable>(tree: &sled::Tree) -> Result<Vec<(String, T)>, GetError> {
    match get_all_from_tree(tree).await {
        Ok(binary_data) => {
            let mut all = Vec::new();
            for (binary_key, binary_value) in binary_data {
                // Convert binary key to String
                let key = String::from_utf8(binary_key.to_vec()).map_err(|error| {
                    log_sync(&format!("Db Interaction Error: {}", error));
                    GetError::Deserialize
                })?;

                // Deserialize binary value to T
                let value = T::from_bin(binary_value).map_err(|error| {
                    log_sync(&format!("Db Interaction Error: {}", error));
                    GetError::Deserialize
                })?;

                all.push((key, value));
            }
            Ok(all)
        }
        Err(error) => Err(match error {
            DatabaseError::NotFound => GetError::NotFound,
            _ => GetError::Database,
        }),
    }
}

pub async fn get<T: Serializable>(tree: &Tree, key: &str) -> Result<T, GetError> {
    match get_from_tree(tree, key).await {
        Ok(binary_data) => T::from_bin(binary_data).map_err(|error| {
            log_sync(&format!("Db Interaction Error: {}", error));
            GetError::Deserialize
        }),
        Err(error) => match error {
            DatabaseError::NotFound => Err(GetError::NotFound),
            _ => Err(GetError::Database),
        },
    }
}

async fn set_to_tree(db: &Tree, key: &str, bin: Vec<u8>) -> Result<(), DatabaseError> {
    match db.insert(key, bin) {
        Ok(_) => Ok(()),
        Err(error) => {
            log_sync(&format!("Db Interaction Error: {}", error));
            Err(DatabaseError::Set)
        }
    }
}

pub async fn set<T: Serializable>(tree: &Tree, key: &str, data: T) -> Result<(), SetError> {
    let binary_data = T::to_bin(&data).map_err(|error| {
        log_sync(&format!("Db Interaction Error: {}", error));
        SetError::Serialize
    })?;
    set_to_tree(tree, key, binary_data)
        .await
        .map_err(|_| SetError::Database)?;
    Ok(())
}

pub async fn delete(tree: &Tree, key: &str) -> Result<(), DeleteError> {
    match tree.remove(key) {
        Ok(result) => match result {
            Some(_deleted_value) => Ok(()),
            None => Err(DeleteError::NotFound),
        },
        Err(error) => {
            log_sync(&format!("Db Interaction Error: {}", error));
            Err(DeleteError::NoDelete)
        }
    }
}

