mod download_files;
mod downloads;
mod model_cards;
mod models;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use download_files::*;
pub use downloads::*;
pub use model_cards::*;
pub use models::*;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Author {
    pub name: String,
    pub url: String,
    pub description: String,
}

impl From<Author> for moly_protocol::data::Author {
    fn from(value: Author) -> Self {
        moly_protocol::data::Author {
            name: value.name,
            url: value.url,
            description: value.description,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteFile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub quantization: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub download: HashMap<String, String>,
}
