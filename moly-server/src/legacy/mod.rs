use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use moly_protocol::data::{DownloadedFile, FileId, Model, PendingDownload};
use moly_protocol::open_ai::{ChatRequestData, ChatResponse};
use moly_protocol::protocol::{
    Command, FileDownloadResponse, LoadModelOptions, LoadModelResponse, LocalServerConfig,
    LocalServerResponse,
};

use crate::models;
use crate::store::index::ModelCardManager;

mod llama_api_server;

#[derive(Clone, Debug)]
enum ModelManagementCommand {
    GetFeaturedModels(Sender<anyhow::Result<Vec<Model>>>),
    SearchModels(String, Sender<anyhow::Result<Vec<Model>>>),
    DownloadFile(FileId, Sender<anyhow::Result<FileDownloadResponse>>),
    PauseDownload(FileId, Sender<anyhow::Result<()>>),
    CancelDownload(FileId, Sender<anyhow::Result<()>>),
    GetCurrentDownloads(Sender<anyhow::Result<Vec<PendingDownload>>>),
    GetDownloadedFiles(Sender<anyhow::Result<Vec<DownloadedFile>>>),
    DeleteFile(FileId, Sender<anyhow::Result<()>>),
    ChangeModelsLocation(PathBuf),
}

#[derive(Clone, Debug)]
enum ModelInteractionCommand {
    LoadModel(
        FileId,
        LoadModelOptions,
        Sender<anyhow::Result<LoadModelResponse>>,
    ),
    EjectModel(Sender<anyhow::Result<()>>),
    Chat(ChatRequestData, Sender<anyhow::Result<ChatResponse>>),
    StopChatCompletion(Sender<anyhow::Result<()>>),
    // Command to start a local server to interact with chat models
    StartLocalServer(
        LocalServerConfig,
        Sender<anyhow::Result<LocalServerResponse>>,
    ),
    // Command to stop the local server
    StopLocalServer(Sender<anyhow::Result<()>>),
}

#[derive(Clone, Debug)]
enum BuiltInCommand {
    Model(ModelManagementCommand),
    Interaction(ModelInteractionCommand),
}

impl From<Command> for BuiltInCommand {
    fn from(value: Command) -> Self {
        match value {
            Command::GetFeaturedModels(tx) => {
                Self::Model(ModelManagementCommand::GetFeaturedModels(tx))
            }
            Command::SearchModels(request, tx) => {
                Self::Model(ModelManagementCommand::SearchModels(request, tx))
            }
            Command::DownloadFile(file_id, tx) => {
                Self::Model(ModelManagementCommand::DownloadFile(file_id, tx))
            }
            Command::PauseDownload(file_id, tx) => {
                Self::Model(ModelManagementCommand::PauseDownload(file_id, tx))
            }
            Command::CancelDownload(file_id, tx) => {
                Self::Model(ModelManagementCommand::CancelDownload(file_id, tx))
            }
            Command::DeleteFile(file_id, tx) => {
                Self::Model(ModelManagementCommand::DeleteFile(file_id, tx))
            }
            Command::GetCurrentDownloads(tx) => {
                Self::Model(ModelManagementCommand::GetCurrentDownloads(tx))
            }
            Command::GetDownloadedFiles(tx) => {
                Self::Model(ModelManagementCommand::GetDownloadedFiles(tx))
            }
            Command::LoadModel(file_id, options, tx) => {
                Self::Interaction(ModelInteractionCommand::LoadModel(file_id, options, tx))
            }
            Command::EjectModel(tx) => Self::Interaction(ModelInteractionCommand::EjectModel(tx)),
            Command::Chat(request, tx) => {
                Self::Interaction(ModelInteractionCommand::Chat(request, tx))
            }
            Command::StopChatCompletion(tx) => {
                Self::Interaction(ModelInteractionCommand::StopChatCompletion(tx))
            }
            Command::StartLocalServer(config, tx) => {
                Self::Interaction(ModelInteractionCommand::StartLocalServer(config, tx))
            }
            Command::StopLocalServer(tx) => {
                Self::Interaction(ModelInteractionCommand::StopLocalServer(tx))
            }
            Command::ChangeModelsDir(path) => {
                Self::Model(ModelManagementCommand::ChangeModelsLocation(path))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum DownloadControlCommand {
    Stop(FileId),
}

pub type LlamaEdgeApiServerBackend = BackendImpl<llama_api_server::LLamaEdgeApiServer>;

pub trait BackendModel: Sized {
    fn new_or_reload(
        async_rt: &tokio::runtime::Runtime,
        old_model: Option<Self>,
        file: models::DownloadedFile,
        options: LoadModelOptions,
        tx: Sender<anyhow::Result<LoadModelResponse>>,
        embedding: Option<(PathBuf, u64)>,
    ) -> Self;
    fn chat(
        &self,
        async_rt: &tokio::runtime::Runtime,
        data: ChatRequestData,
        tx: Sender<anyhow::Result<ChatResponse>>,
    ) -> bool;
    fn stop_chat(&self, async_rt: &tokio::runtime::Runtime);
    fn stop(self, async_rt: &tokio::runtime::Runtime);
}

pub struct BackendImpl<Model: BackendModel> {
    db_conn: Arc<Mutex<rusqlite::Connection>>,
    model_indexs: ModelCardManager,
    #[allow(unused)]
    app_data_dir: PathBuf,
    models_dir: PathBuf,
    pub rx: Receiver<Command>,
    download_tx: tokio::sync::mpsc::UnboundedSender<(
        models::Model,
        models::DownloadedFile,
        models::RemoteFile,
        Sender<anyhow::Result<FileDownloadResponse>>,
    )>,
    model: Option<Model>,

    #[allow(unused)]
    async_rt: tokio::runtime::Runtime,
    control_tx: tokio::sync::broadcast::Sender<DownloadControlCommand>,
}

impl<Model: BackendModel + Send + 'static> BackendImpl<Model> {
    /// # Arguments
    /// * `app_data_dir` - The directory where application data should be stored.
    /// * `models_dir` - The directory where models should be downloaded.
    /// * `max_download_threads` - Maximum limit on simultaneous file downloads.
    pub fn build_command_sender<A: AsRef<Path>, M: AsRef<Path>>(
        app_data_dir: A,
        models_dir: M,
        max_download_threads: usize,
    ) -> Sender<Command> {
        let app_data_dir = app_data_dir.as_ref().to_path_buf();

        log::info!("build by app_data_dir: {:?}", app_data_dir);

        wasmedge_sdk::plugin::PluginManager::load(None).unwrap();
        std::fs::create_dir_all(&app_data_dir).unwrap_or_else(|_| {
            panic!(
                "Failed to create the Moly app data directory at {:?}",
                app_data_dir
            )
        });

        let model_indexs = crate::store::index::sync_model_cards_repo(&app_data_dir);
        let model_indexs = match model_indexs {
            Ok(model_indexs) => {
                log::info!("sync model cards repo success");
                model_indexs
            }
            Err(e) => {
                log::error!("sync model cards repo error: {e}");
                ModelCardManager::empty(app_data_dir.clone())
            }
        };

        let db_conn = rusqlite::Connection::open(app_data_dir.join("data.sqlite")).unwrap();

        // TODO Reorganize these bunch of functions, needs a little more of thought
        models::create_table_models(&db_conn).unwrap();
        models::create_table_download_files(&db_conn).unwrap();

        let db_conn = Arc::new(Mutex::new(db_conn));

        let (tx, rx) = std::sync::mpsc::channel();

        let async_rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let (control_tx, _control_rx) = tokio::sync::broadcast::channel(100);
        let (download_tx, download_rx) = tokio::sync::mpsc::unbounded_channel();

        {
            use crate::store::download::ModelFileDownloader;

            let client = reqwest::Client::new();
            let downloader = ModelFileDownloader::new(
                client,
                db_conn.clone(),
                control_tx.clone(),
                model_indexs.country_code.clone(),
                0.1,
            );
            async_rt.spawn(ModelFileDownloader::run_loop(
                downloader,
                max_download_threads.max(3),
                download_rx,
            ));
        }

        let mut backend = Self {
            db_conn,
            model_indexs,
            app_data_dir,
            models_dir: models_dir.as_ref().into(),
            rx,
            download_tx,
            model: None,
            async_rt,
            control_tx,
        };

        std::thread::spawn(move || {
            backend.run_loop();
        });
        tx
    }

    fn handle_command(&mut self, built_in_cmd: BuiltInCommand) {
        match built_in_cmd {
            BuiltInCommand::Model(file) => match file {
                ModelManagementCommand::GetFeaturedModels(tx) => {
                    let res = self.model_indexs.get_featured_model(100, 0);
                    match res {
                        Ok(indexs) => {
                            let mut models = Vec::new();
                            for index in indexs {
                                if let Ok(card) = self.model_indexs.load_model_card(&index) {
                                    models.push(card);
                                }
                            }

                            let db_conn = self.db_conn.lock().unwrap();
                            let models = models::ModelCard::to_model(&models, &db_conn)
                                .map_err(|e| anyhow::anyhow!("get featured error: {e}"));

                            let _ = tx.send(models);
                        }
                        Err(err) => {
                            let _ =
                                tx.send(Err(anyhow::anyhow!("get featured models error: {err}")));
                        }
                    }
                }
                ModelManagementCommand::SearchModels(search_text, tx) => {
                    let res = self.model_indexs.search(&search_text, 100, 0);
                    match res {
                        Ok(indexs) => {
                            log::debug!("search models: {}", indexs.len());
                            let db_conn = self.db_conn.lock().unwrap();

                            let mut models = Vec::new();
                            for index in indexs {
                                match self.model_indexs.load_model_card(&index) {
                                    Ok(card) => {
                                        models.push(card);
                                    }
                                    Err(err) => {
                                        log::error!("load model card {} error: {err}", index.id);
                                    }
                                }
                            }

                            let models = models::ModelCard::to_model(&models, &db_conn)
                                .map_err(|e| anyhow::anyhow!("search models error: {e}"));

                            let _ = tx.send(models);
                        }
                        Err(err) => {
                            let _ = tx.send(Err(anyhow::anyhow!("search models error: {err}")));
                        }
                    }
                }
                ModelManagementCommand::DownloadFile(file_id, tx) => {
                    //search model from remote
                    let mut search_model_from_remote = || -> anyhow::Result<(models::Model, models::DownloadedFile, models::RemoteFile)> {
                        let (model_id, file) = file_id
                            .split_once('#')
                            .ok_or_else(|| anyhow::anyhow!("Illegal file_id"))?;

                        let index = self.model_indexs.get_index_by_id(model_id).ok_or(anyhow::anyhow!("No model found"))?.clone();
                        let remote_model = self.model_indexs.load_model_card(&index)?;

                        let remote_file = remote_model
                            .files
                            .into_iter()
                            .find(|f| f.name == file)
                            .ok_or_else(|| anyhow::anyhow!("file not found"))?;

                        let remote_file_ = remote_file.clone();

                        let download_model = models::Model {
                            id: Arc::new(remote_model.id),
                            name: remote_model.name,
                            summary: remote_model.summary,
                            size: remote_model.size,
                            requires: remote_model.requires,
                            architecture: remote_model.architecture,
                            released_at: remote_model.released_at,
                            prompt_template: remote_model.prompt_template.clone(),
                            reverse_prompt: remote_model.reverse_prompt.clone(),
                            author: Arc::new(models::Author {
                                name: remote_model.author.name,
                                url: remote_model.author.url,
                                description: remote_model.author.description,
                            }),
                            like_count: remote_model.like_count,
                            download_count: remote_model.download_count,
                        };

                        let download_file = models::DownloadedFile {
                            id: Arc::new(file_id.clone()),
                            model_id: model_id.to_string(),
                            name: file.to_string(),
                            size: remote_file.size,
                            quantization: remote_file.quantization,
                            prompt_template: remote_model.prompt_template,
                            reverse_prompt: remote_model.reverse_prompt,
                            context_size: remote_model.context_size,
                            downloaded: false,
                            file_size: 0,
                            download_dir: self.models_dir.to_string_lossy().to_string(),
                            downloaded_at: Utc::now(),
                            tags: remote_file.tags,
                            featured: false,
                            sha256: remote_file.sha256.unwrap_or_default(),
                        };

                        Ok((download_model,download_file,remote_file_))
                    };

                    match search_model_from_remote() {
                        Ok((model, file, remote_file)) => {
                            let _ = self.download_tx.send((model, file, remote_file, tx));
                        }
                        Err(err) => {
                            let _ = tx.send(Err(err));
                        }
                    }
                }

                ModelManagementCommand::PauseDownload(file_id, tx) => {
                    let _ = self.control_tx.send(DownloadControlCommand::Stop(file_id));
                    let _ = tx.send(Ok(()));
                }

                ModelManagementCommand::CancelDownload(file_id, tx) => {
                    let file_id_ = file_id.clone();
                    let _ = self.control_tx.send(DownloadControlCommand::Stop(file_id_));

                    {
                        let conn = self.db_conn.lock().unwrap();
                        let _ = models::DownloadedFile::remove(&file_id, &conn);
                    }
                    let _ = crate::store::remove_downloaded_file(&self.models_dir, file_id);

                    let _ = tx.send(Ok(()));
                }

                ModelManagementCommand::DeleteFile(file_id, tx) => {
                    {
                        let conn = self.db_conn.lock().unwrap();
                        let _ = models::DownloadedFile::remove(&file_id, &conn);
                    }

                    let _ = crate::store::remove_downloaded_file(&self.models_dir, file_id);
                    let _ = tx.send(Ok(()));
                }

                ModelManagementCommand::GetDownloadedFiles(tx) => {
                    let downloads = {
                        let conn = self.db_conn.lock().unwrap();
                        crate::store::get_downloaded_files(&conn)
                            .map_err(|e| anyhow::anyhow!("get download file error: {e}"))
                    };

                    let _ = tx.send(downloads);
                }

                ModelManagementCommand::GetCurrentDownloads(tx) => {
                    let pending_downloads = {
                        let conn = self.db_conn.lock().unwrap();
                        crate::store::get_pending_downloads(&conn)
                            .map_err(|e| anyhow::anyhow!("get pending download file error: {e}"))
                    };
                    let _ = tx.send(pending_downloads);
                }

                ModelManagementCommand::ChangeModelsLocation(path) => self.update_models_dir(path),
            },
            BuiltInCommand::Interaction(model_cmd) => match model_cmd {
                ModelInteractionCommand::LoadModel(file_id, options, tx) => {
                    let conn = self.db_conn.lock().unwrap();
                    let download_file = models::DownloadedFile::get_by_id(&conn, &file_id);

                    match download_file {
                        Ok(file) => {
                            nn_preload_file(&file, self.model_indexs.embedding_model());
                            let old_model = self.model.take();

                            self.model = Some(Model::new_or_reload(
                                &self.async_rt,
                                old_model,
                                file,
                                options,
                                tx,
                                self.model_indexs.embedding_model(),
                            ));
                        }
                        Err(err) => {
                            let _ = tx.send(Err(anyhow::anyhow!("Load model error: {err}")));
                        }
                    }
                }
                ModelInteractionCommand::EjectModel(tx) => {
                    if let Some(model) = self.model.take() {
                        model.stop(&self.async_rt);
                    }
                    let _ = tx.send(Ok(()));
                }
                ModelInteractionCommand::Chat(data, tx) => {
                    if let Some(model) = &self.model {
                        model.chat(&self.async_rt, data, tx);
                    } else {
                        let _ = tx.send(Err(anyhow::anyhow!("Model not loaded")));
                    }
                }
                ModelInteractionCommand::StopChatCompletion(tx) => {
                    if let Some(ref model) = self.model {
                        model.stop_chat(&self.async_rt);
                    }
                    let _ = tx.send(Ok(()));
                }
                ModelInteractionCommand::StartLocalServer(_, _) => todo!(),
                ModelInteractionCommand::StopLocalServer(_) => todo!(),
            },
        }
    }

    pub fn update_models_dir<M: AsRef<Path>>(&mut self, models_dir: M) {
        self.models_dir = models_dir.as_ref().to_path_buf();
    }

    fn run_loop(&mut self) {
        while let Ok(cmd) = self.rx.recv() {
            self.handle_command(cmd.into());
        }

        log::debug!("BackendImpl stop");
    }
}

pub fn nn_preload_file(file: &models::DownloadedFile, embedding: Option<(PathBuf, u64)>) {
    let file_path = Path::new(&file.download_dir)
        .join(&file.model_id)
        .join(&file.name);

    let preloads = wasmedge_sdk::plugin::NNPreload::new(
        "moly-chat",
        wasmedge_sdk::plugin::GraphEncoding::GGML,
        wasmedge_sdk::plugin::ExecutionTarget::AUTO,
        &file_path,
    );

    let mut preload_vec = vec![preloads];
    if let Some((embedding_path, _)) = embedding {
        let preloads = wasmedge_sdk::plugin::NNPreload::new(
            "moly-embedding",
            wasmedge_sdk::plugin::GraphEncoding::GGML,
            wasmedge_sdk::plugin::ExecutionTarget::AUTO,
            &embedding_path,
        );
        preload_vec.push(preloads);
    }

    wasmedge_sdk::plugin::PluginManager::nn_preload(preload_vec);
}
