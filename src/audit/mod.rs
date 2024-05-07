use std::{
    env, fs};

use crate::common::get_unix_time_millis;



/// Internal logging function used to log errors. 
/// Can be disabled by setting environment variable ACCEPTEVM to 0
pub fn log_sync<'a>(data: &'a str) -> () {
    match env::var("ACCEPTEVM_LOGS") {
        Ok(value) => {
            if value == "0".to_string() {
                return;
            }
        }
        Err(_error) => {
            return;
        }
    }
    let path = format!("{}.log.txt", get_unix_time_millis());
    let write_result = fs::write(path, format!("{}\n", data));
    match write_result {
        Ok(()) => {}
        Err(error) => {
            panic!("LOGGER FAILURE! COULD NOT LOG DATA! {}", error)
        }
    }
}
