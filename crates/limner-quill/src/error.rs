use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid JSON: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("not a Quill project: {0}")]
    NotAProject(String),
    #[error("unsupported or damaged Quill project: {0}")]
    Format(String),
}

impl ImportError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        ImportError::Io {
            path: path.into(),
            source,
        }
    }
}
