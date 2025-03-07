use std::sync::{Arc, Mutex};

use crate::capture::{CaptureHandler, InitError};

pub fn register_handler<T>(handler: Arc<Mutex<T>>) -> Result<(), InitError>
where
    T: CaptureHandler,
{
    Ok(())
}
