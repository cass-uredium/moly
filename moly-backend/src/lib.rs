mod controllers;
mod models;
mod services;

use std::path::Path;
use std::sync::mpsc;

use moly_protocol::protocol::Command;

pub struct Backend {
    pub command_sender: mpsc::Sender<Command>,
}

impl Backend {
    /// # Arguments
    /// * `app_data_dir` - The directory where application data should be stored.
    /// * `models_dir` - The directory where models should be downloaded.
    /// * `max_download_threads` - Maximum limit on simultaneous file downloads.
    pub fn new<A: AsRef<Path>, M: AsRef<Path>>(
        app_data_dir: A,
        models_dir: M,
        max_download_threads: usize,
    ) -> Backend {
        #[cfg(debug_assertions)]
        env_logger::init();
        let command_sender = services::LlamaEdgeApiServerBackend::build_command_sender(
            app_data_dir,
            models_dir,
            max_download_threads,
        );
        Backend { command_sender }
    }
}
