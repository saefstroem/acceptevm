use std::{env, fs};
use uuid::Uuid;

/// Internal logging function used to log errors.
/// Can be disabled by setting environment variable ACCEPTEVM to 0
pub fn log_sync(data: &str) {
    match env::var("ACCEPTEVM_LOGS") {
        Ok(value) => {
            if value == *"0" {
                return;
            }
        }
        Err(_error) => {
            return;
        }
    }
    let path = format!("{}.error.log.txt", Uuid::new_v4());
    let write_result = fs::write(path, format!("{}\n", data));
    match write_result {
        Ok(()) => {}
        Err(error) => {
            panic!("LOGGER FAILURE! COULD NOT LOG DATA! {}", error)
        }
    }
}
