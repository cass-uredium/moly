use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{Author, DownloadedFile, RemoteFile};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelCard {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub requires: String,
    #[serde(default)]
    pub architecture: String,
    pub released_at: DateTime<Utc>,
    #[serde(default)]
    pub files: Vec<RemoteFile>,
    pub prompt_template: String,
    pub reverse_prompt: String,
    pub context_size: u64,
    pub author: Author,
    #[serde(default)]
    pub like_count: u32,
    #[serde(default)]
    pub download_count: u32,
    #[serde(default)]
    pub metrics: Option<HashMap<String, f32>>,
}

impl ModelCard {
    pub fn to_model(
        remote_models: &[Self],
        conn: &rusqlite::Connection,
    ) -> rusqlite::Result<Vec<moly_protocol::data::Model>> {
        let model_ids = remote_models
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>();
        let files = DownloadedFile::get_downloaded_by_models(conn, &model_ids)?;

        fn to_file(
            model_id: &str,
            remote_files: &[RemoteFile],
            save_files: &HashMap<Arc<String>, DownloadedFile>,
        ) -> rusqlite::Result<Vec<moly_protocol::data::File>> {
            let mut files = vec![];
            for remote_f in remote_files {
                let file_id = format!("{}#{}", model_id, remote_f.name);
                let downloaded_path = save_files.get(&file_id).map(|file| {
                    let file_path = Path::new(&file.download_dir)
                        .join(&file.model_id)
                        .join(&file.name);
                    file_path
                        .to_str()
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                });

                let file = moly_protocol::data::File {
                    id: file_id,
                    name: remote_f.name.clone(),
                    size: remote_f.size.clone(),
                    quantization: remote_f.quantization.clone(),
                    downloaded: downloaded_path.is_some(),
                    downloaded_path,
                    tags: remote_f.tags.clone(),
                    featured: false,
                };

                files.push(file);
            }

            Ok(files)
        }

        let mut models = Vec::with_capacity(remote_models.len());

        for remote_m in remote_models {
            let model = moly_protocol::data::Model {
                id: remote_m.id.clone(),
                name: remote_m.name.clone(),
                summary: remote_m.summary.clone(),
                size: remote_m.size.clone(),
                requires: remote_m.requires.clone(),
                architecture: remote_m.architecture.clone(),
                released_at: remote_m.released_at.clone(),
                files: to_file(&remote_m.id, &remote_m.files, &files)?,
                author: moly_protocol::data::Author {
                    name: remote_m.author.name.clone(),
                    url: remote_m.author.url.clone(),
                    description: remote_m.author.description.clone(),
                },
                like_count: remote_m.like_count.clone(),
                download_count: remote_m.download_count.clone(),
                metrics: remote_m.metrics.clone().unwrap_or_default(),
            };

            models.push(model);
        }

        Ok(models)
    }
}
