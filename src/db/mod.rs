use sled::Tree;
use thiserror::Error;
use crate::invoice::Invoice;

#[derive(Error, Debug)]
pub enum DatabaseError {
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
    #[error("Database internal error: {0}")]
    SledError(#[from] sled::Error),
}


/// Retrieve a value by key from a tree.
async fn get_from_tree(db: &Tree, key: &str) -> Result<Vec<u8>, DatabaseError> {
    Ok(db.get(key)?.ok_or(DatabaseError::NotFound)?.to_vec())
}
/// Retrieve all key,value pairs from a specified tree
async fn get_all_from_tree(db: &Tree) -> Result<Vec<(Vec<u8>, Vec<u8>)>, DatabaseError> {
    db.iter()
        .map(|res| {
            res.map_err(|error| {
                log::error!("Db Interaction Error: {}", error);
                DatabaseError::Get
            })
            .map(|(key, value)| (key.to_vec(), value.to_vec()))
        })
        .collect()
}

/// Retrieve the last added item to the tree
async fn get_last_from_tree(db: &Tree) -> Result<(Vec<u8>, Vec<u8>), DatabaseError> {
    db.last()?
        .map(|(key, value)| (key.to_vec(), value.to_vec()))
        .ok_or(DatabaseError::NotFound)
}

/// Wrapper for retrieving the last added item to the tree
pub async fn get_last(tree: &sled::Tree) -> Result<(String, Invoice), DatabaseError> {
    let binary_data = get_last_from_tree(tree).await?;
    // Convert binary key to String
    let key = String::from_utf8(binary_data.0).map_err(|error| {
        log::error!("Db Interaction Error: {}", error);
        DatabaseError::Deserialize
    })?;

    // Deserialize binary value to T
    let value = bincode::deserialize::<Invoice>(&binary_data.1).map_err(|error| {
        log::error!("Db Interaction Error: {}", error);
        DatabaseError::Deserialize
    })?;
    Ok((key, value))
}

/// Wrapper for retrieving all key value pairs from a tree
pub async fn get_all(tree: &sled::Tree) -> Result<Vec<(String, Invoice)>, DatabaseError> {
    let binary_data = get_all_from_tree(tree).await?;
    let mut all = Vec::with_capacity(binary_data.len());
    for (binary_key, binary_value) in binary_data {
        // Convert binary key to String
        let key = String::from_utf8(binary_key.to_vec()).map_err(|error| {
            log::error!("Db Interaction Error: {}", error);
            DatabaseError::Deserialize
        })?;

        // Deserialize binary value to invoice
        let value = bincode::deserialize::<Invoice>(&binary_value).map_err(|error| {
            log::error!("Db Interaction Error: {}", error);
            DatabaseError::Deserialize
        })?;

        all.push((key, value));
    }
    Ok(all)
}

/// Wrapper for retrieving a value from a tree
pub async fn get(tree: &Tree, key: &str) -> Result<Invoice, DatabaseError> {
    let binary_data = get_from_tree(tree, key).await?;
    bincode::deserialize::<Invoice>(&binary_data).map_err(|error| {
        log::error!("Db Interaction Error: {}", error);
        DatabaseError::Deserialize
    })
}

/// Sets a value to a tree
async fn set_to_tree(db: &Tree, key: &str, bin: Vec<u8>) -> Result<(), DatabaseError> {
    match db.insert(key, bin) {
        Ok(_) => Ok(()),
        Err(error) => {
            log::error!("Db Interaction Error: {}", error);
            Err(DatabaseError::Set)
        }
    }
}

/// Wrapper for setting a value to a tree
pub async fn set(tree: &Tree, key: &str, data: &Invoice) -> Result<(), DatabaseError> {
    let binary_data = bincode::serialize::<Invoice>(data).map_err(|error| {
        log::error!("Db Interaction Error: {}", error);
        DatabaseError::Serialize
    })?;
    set_to_tree(tree, key, binary_data)
        .await
        .map_err(|_| DatabaseError::Communicate)?;
    Ok(())
}

/// Used to delete from a tree
pub async fn delete(tree: &Tree, key: &str) -> Result<(), DatabaseError> {
    let result = tree.remove(key)?;
    match result {
        Some(_deleted_value) => Ok(()),
        None => Err(DatabaseError::NotFound),
    }
}
