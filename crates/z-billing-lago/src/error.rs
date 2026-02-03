//! Error types for Lago deployment management.

use std::path::PathBuf;

/// Errors that can occur during Lago deployment operations.
#[derive(Debug, thiserror::Error)]
pub enum DeployError {
    /// The Lago repository path does not exist.
    #[error("Lago path does not exist: {0}")]
    PathNotFound(PathBuf),

    /// The docker-compose.yml file was not found in the Lago directory.
    #[error("docker-compose.yml not found in Lago directory: {0}")]
    ComposeFileNotFound(PathBuf),

    /// Docker Compose command failed to execute.
    #[error("Failed to execute docker compose: {0}")]
    CommandExecution(#[from] std::io::Error),

    /// Docker Compose command returned a non-zero exit code.
    #[error("Docker compose command failed with exit code {exit_code}: {stderr}")]
    CommandFailed {
        /// The exit code returned by the command.
        exit_code: i32,
        /// The stderr output from the command.
        stderr: String,
    },

    /// Failed to parse command output.
    #[error("Failed to parse command output: {0}")]
    ParseError(String),

    /// Docker is not installed or not running.
    #[error("Docker is not available: {0}")]
    DockerNotAvailable(String),
}
