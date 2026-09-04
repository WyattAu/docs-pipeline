//! Error types for the docs pipeline.

use thiserror::Error;

/// Error type for the docs pipeline.
#[derive(Error, Debug)]
pub enum Error {
    /// Error during markdown parsing
    #[error("Markdown parsing error: {0}")]
    MarkdownParse(String),

    /// Error during markdown to HTML conversion
    #[error("Markdown to HTML conversion error: {0}")]
    MarkdownToHtml(String),

    /// Syntax highlighting error
    #[error("Syntax highlighting error: {0}")]
    SyntaxHighlight(String),

    /// Unsupported language for syntax highlighting
    #[error("Unsupported language for syntax highlighting: {0}")]
    UnsupportedLanguage(String),

    /// Language parser not available
    #[error("Language parser not available for: {0}")]
    LanguageParserUnavailable(String),

    /// LaTeX rendering error
    #[error("LaTeX rendering error: {0}")]
    LatexRender(String),

    /// Invalid LaTeX syntax
    #[error("Invalid LaTeX syntax: {0}")]
    InvalidLatex(String),

    /// Unknown LaTeX command
    #[error("Unknown LaTeX command: {0}")]
    UnknownLatexCommand(String),

    /// Input/output error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Timeout
    #[error("Operation timed out after {0}ms")]
    Timeout(u64),

    /// Internal error
    #[error("Internal pipeline error: {0}")]
    Internal(String),
}

impl Error {
    /// Create a new markdown parsing error
    pub fn markdown_parse<S: Into<String>>(message: S) -> Self {
        Error::MarkdownParse(message.into())
    }

    /// Create a new markdown to HTML conversion error
    pub fn markdown_to_html<S: Into<String>>(message: S) -> Self {
        Error::MarkdownToHtml(message.into())
    }

    /// Create a new syntax highlighting error
    pub fn syntax_highlight<S: Into<String>>(message: S) -> Self {
        Error::SyntaxHighlight(message.into())
    }

    /// Create a new unsupported language error
    pub fn unsupported_language<S: Into<String>>(language: S) -> Self {
        Error::UnsupportedLanguage(language.into())
    }

    /// Create a new language parser unavailable error
    pub fn language_parser_unavailable<S: Into<String>>(language: S) -> Self {
        Error::LanguageParserUnavailable(language.into())
    }

    /// Create a new LaTeX rendering error
    pub fn latex_render<S: Into<String>>(message: S) -> Self {
        Error::LatexRender(message.into())
    }

    /// Create a new invalid LaTeX error
    pub fn invalid_latex<S: Into<String>>(message: S) -> Self {
        Error::InvalidLatex(message.into())
    }

    /// Create a new unknown LaTeX command error
    pub fn unknown_latex_command<S: Into<String>>(command: S) -> Self {
        Error::UnknownLatexCommand(command.into())
    }

    /// Create a new serialization error
    pub fn serialization<S: Into<String>>(message: S) -> Self {
        Error::Serialization(message.into())
    }

    /// Create a new deserialization error
    pub fn deserialization<S: Into<String>>(message: S) -> Self {
        Error::Deserialization(message.into())
    }

    /// Create a new invalid input error
    pub fn invalid_input<S: Into<String>>(message: S) -> Self {
        Error::InvalidInput(message.into())
    }

    /// Create a new timeout error
    pub fn timeout(duration_ms: u64) -> Self {
        Error::Timeout(duration_ms)
    }

    /// Create a new internal error
    pub fn internal<S: Into<String>>(message: S) -> Self {
        Error::Internal(message.into())
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Serialization(err.to_string())
    }
}

/// Result type alias for pipeline operations
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::markdown_parse("Invalid markdown");
        assert_eq!(err.to_string(), "Markdown parsing error: Invalid markdown");
    }

    #[test]
    fn test_unsupported_language_display() {
        let err = Error::unsupported_language("brainfuck");
        assert_eq!(
            err.to_string(),
            "Unsupported language for syntax highlighting: brainfuck"
        );
    }
}
