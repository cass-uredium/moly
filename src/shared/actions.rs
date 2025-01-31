use makepad_widgets::{ActionDefaultRef, DefaultNone};
use moly_protocol::data::FileId;

#[derive(Clone, DefaultNone, Debug)]
pub enum ChatAction {
    Start(FileId),
    None,
}

#[derive(Clone, DefaultNone, Debug)]
pub enum DownloadAction {
    Play(FileId),
    Pause(FileId),
    Cancel(FileId),
    None,
}
