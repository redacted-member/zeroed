//! Kernel error type.

use thiserror::Error;

/// Every fallible kernel entry point returns this error. Its message is
/// what the user reads, in plain words: the kind of error travels as
/// [`CoreError::code`], so no message starts with it.
#[derive(Debug, Error)]
pub enum CoreError {
    /// The command name is not one the kernel knows.
    #[error("the app does not know the command {0}")]
    UnknownCommand(String),

    /// The argument JSON did not match the command's expected shape.
    #[error("the settings sent to {command} do not read: {message}")]
    InvalidArgs {
        /// Command that was called.
        command: String,
        /// Human readable reason, taken from the deserialiser.
        message: String,
    },

    /// A value was well formed JSON but not a legal document value.
    #[error("{0}")]
    Validation(String),

    /// A reply could not be encoded as JSON.
    #[error("the answer could not be written out: {0}")]
    Encode(#[from] serde_json::Error),

    /// A geometry operation was given input it cannot work with.
    #[error("{0}")]
    Geometry(String),

    /// A font could not be found, read or used for layout.
    #[error("{0}")]
    Font(String),

    /// The operation is understood but not implemented in this build.
    #[error("{0}")]
    Unsupported(String),

    /// A file could not be read or written.
    #[error("could not read or write {path}: {message}")]
    Io {
        /// File or directory the operation was about.
        path: String,
        /// Human readable reason, taken from the operating system.
        message: String,
    },

    /// A file was read but is not the format it claims to be.
    #[error("the {format} file could not be read or written: {message}")]
    Format {
        /// Format that was expected, for instance `SVG`.
        format: String,
        /// What was wrong with the file.
        message: String,
    },

    /// A command stopped on a fault in the kernel itself (a panic), caught
    /// so that it does not take the app down.
    #[error("something went wrong inside the app: {0}")]
    Internal(String),

    /// The user stopped the command before it finished; nothing changed.
    #[error("stopped before it finished")]
    Cancelled,
}

impl CoreError {
    /// Short machine readable code, sent to the front end with the message.
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::UnknownCommand(_) => "unknown_command",
            CoreError::InvalidArgs { .. } => "invalid_args",
            CoreError::Validation(_) => "validation",
            CoreError::Encode(_) => "encode",
            CoreError::Geometry(_) => "geometry",
            CoreError::Font(_) => "font",
            CoreError::Unsupported(_) => "unsupported",
            CoreError::Io { .. } => "io",
            CoreError::Format { .. } => "format",
            CoreError::Internal(_) => "internal",
            CoreError::Cancelled => "cancelled",
        }
    }

    /// Builds an [`CoreError::InvalidArgs`] from a deserialiser failure.
    pub fn invalid_args(command: &str, message: impl std::fmt::Display) -> Self {
        CoreError::InvalidArgs {
            command: command.to_string(),
            message: message.to_string(),
        }
    }

    /// Builds a [`CoreError::Validation`] error.
    pub fn validation(message: impl std::fmt::Display) -> Self {
        CoreError::Validation(message.to_string())
    }

    /// Builds a [`CoreError::Geometry`] error.
    pub fn geometry(message: impl std::fmt::Display) -> Self {
        CoreError::Geometry(message.to_string())
    }

    /// Builds a [`CoreError::Font`] error.
    pub fn font(message: impl std::fmt::Display) -> Self {
        CoreError::Font(message.to_string())
    }

    /// Builds a [`CoreError::Unsupported`] error.
    pub fn unsupported(message: impl std::fmt::Display) -> Self {
        CoreError::Unsupported(message.to_string())
    }

    /// Builds a [`CoreError::Format`] error naming the format that was
    /// expected.
    pub fn format(format: impl std::fmt::Display, message: impl std::fmt::Display) -> Self {
        CoreError::Format {
            format: format.to_string(),
            message: message.to_string(),
        }
    }

    /// Builds a [`CoreError::Io`] error naming the file it happened on.
    pub fn io(path: impl std::fmt::Display, message: impl std::fmt::Display) -> Self {
        CoreError::Io {
            path: path.to_string(),
            message: message.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CoreError;

    #[test]
    fn messages_read_as_plain_words_without_the_kind_in_front() {
        let refused = "select the closed vectors to use";
        for error in [
            CoreError::validation(refused),
            CoreError::geometry(refused),
            CoreError::font(refused),
            CoreError::unsupported(refused),
        ] {
            assert_eq!(error.to_string(), refused, "{}", error.code());
        }
        let file = CoreError::io("/tmp/a.carve", "permission denied").to_string();
        assert_eq!(
            file,
            "could not read or write /tmp/a.carve: permission denied"
        );
        let format = CoreError::format("PDF", "the file has no pages").to_string();
        assert_eq!(
            format,
            "the PDF file could not be read or written: the file has no pages"
        );
    }
}
