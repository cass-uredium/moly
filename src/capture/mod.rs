use std::fmt;

mod sys;

pub fn register_handler<T>(handler: T)
where
    T: CaptureHandler,
{
    sys::register_handler(handler);
}

#[derive(Debug, Clone)]
pub struct CaptureData {
    contents: String,
    kind: CaptureKind,
}

impl CaptureData {
    pub(crate) fn new(data: String, kind: CaptureKind) -> Self {
        Self {
            contents: data,
            kind,
        }
    }

    pub fn contents(&self) -> &str {
        &self.contents
    }

    pub fn kind(&self) -> CaptureKind {
        self.kind
    }
}

#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum CaptureKind {
    Text,
}

#[derive(Debug)]
pub struct CaptureError {
    message: String,
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "capture error: {}", self.message)
    }
}

impl std::error::Error for CaptureError {}

pub trait CaptureHandler: 'static + Send + Sync {
    fn capture(&self, data: CaptureData);

    fn error(&self, error: CaptureError);
}
