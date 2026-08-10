use serde::Serialize;

/// Every error that can cross the IPC boundary.
///
/// The frontend receives `{ "kind": "...", "message": "..." }` so it can show a
/// human-readable message plus a "copy details" affordance without parsing
/// free-form strings.
#[derive(Debug, thiserror::Error)]
pub enum NimbusError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("malformed JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("config directory could not be resolved on this system")]
    NoConfigDir,

    #[error(
        "config file is from a newer version ({found}) than this launcher supports ({supported})"
    )]
    ConfigTooNew { found: u32, supported: u32 },

    #[error("{0}")]
    Invalid(String),

    /// The user aborted a long-running operation (install, import, ...).
    /// The frontend treats this as a normal outcome, not a failure.
    #[error("Операция отменена")]
    Cancelled,

    /// The instance is still running and the requested operation would
    /// corrupt it (delete, duplicate, mod changes, ...).
    #[error("Сборка сейчас запущена — закройте игру и повторите")]
    Running,

    /// An HTTP response with a non-success status. `retriable` is set for
    /// 5xx and network errors only; 4xx are permanent failures.
    #[error("HTTP {status} from {url}")]
    Http {
        status: u16,
        url: String,
        retriable: bool,
    },

    #[error("network error: {0}")]
    Network(String),

    #[error("{algorithm} mismatch for {path}: expected {expected}, got {actual}")]
    HashMismatch {
        path: String,
        algorithm: String,
        expected: String,
        actual: String,
    },

    /// An argument placeholder was present in the version JSON but is not
    /// in the known set. Rejecting it early prevents silent broken launches.
    #[error("unknown placeholder ${0} in version arguments")]
    UnknownPlaceholder(String),

    #[error("Java {0} could not be found on this system and could not be downloaded")]
    JavaNotFound(u32),

    #[error("version '{0}' was not found in the version manifest")]
    VersionNotFound(String),

    #[error("zip error: {0}")]
    Zip(String),

    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

impl NimbusError {
    /// Stable machine-readable code. The frontend switches on this instead of
    /// matching on localised message text.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Json(_) => "json",
            Self::NoConfigDir => "noConfigDir",
            Self::ConfigTooNew { .. } => "configTooNew",
            Self::Invalid(_) => "invalid",
            Self::Cancelled => "cancelled",
            Self::Running => "running",
            Self::Http { .. } => "http",
            Self::Network(_) => "network",
            Self::HashMismatch { .. } => "hashMismatch",
            Self::UnknownPlaceholder(_) => "unknownPlaceholder",
            Self::JavaNotFound(_) => "javaNotFound",
            Self::VersionNotFound(_) => "versionNotFound",
            Self::Zip(_) => "zip",
            Self::Reqwest(_) => "network",
        }
    }

    /// True when retrying the same operation may succeed.
    pub fn is_retriable(&self) -> bool {
        match self {
            Self::Http { retriable, .. } => *retriable,
            Self::Network(_) | Self::Reqwest(_) => true,
            _ => false,
        }
    }
}

impl Serialize for NimbusError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("NimbusError", 3)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.serialize_field("retriable", &self.is_retriable())?;
        state.end()
    }
}

pub type Result<T> = std::result::Result<T, NimbusError>;
