//! Docker Compose deployment management for Lago.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;
use tracing::{debug, info, instrument};

use crate::config::LagoConfig;
use crate::error::DeployError;

/// Status of a Docker Compose service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    /// Service is running.
    Running,
    /// Service is stopped/exited.
    Stopped,
    /// Service is starting up.
    Starting,
    /// Service status is unknown.
    Unknown,
}

/// Status of the Lago deployment.
#[derive(Debug, Clone)]
pub struct DeployStatus {
    /// Overall deployment status.
    pub running: bool,
    /// Individual service statuses.
    pub services: Vec<(String, ServiceStatus)>,
}

/// Manages Lago deployment via Docker Compose.
#[derive(Debug, Clone)]
pub struct LagoDeployment {
    /// Path to the cloned Lago repository.
    lago_path: PathBuf,
    /// Optional configuration overrides.
    config: Option<LagoConfig>,
}

impl LagoDeployment {
    /// Create a new Lago deployment manager.
    ///
    /// # Arguments
    ///
    /// * `lago_path` - Path to the cloned Lago repository containing docker-compose.yml
    ///
    /// # Errors
    ///
    /// Returns an error if the path does not exist or doesn't contain docker-compose.yml.
    pub fn new(lago_path: impl AsRef<Path>) -> Result<Self, DeployError> {
        let lago_path = lago_path.as_ref().to_path_buf();

        if !lago_path.exists() {
            return Err(DeployError::PathNotFound(lago_path));
        }

        let compose_file = lago_path.join("docker-compose.yml");
        if !compose_file.exists() {
            // Also check for docker-compose.yaml
            let compose_file_alt = lago_path.join("docker-compose.yaml");
            if !compose_file_alt.exists() {
                return Err(DeployError::ComposeFileNotFound(lago_path));
            }
        }

        Ok(Self {
            lago_path,
            config: None,
        })
    }

    /// Set the configuration for the deployment.
    #[must_use]
    pub fn with_config(mut self, config: LagoConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Start the Lago services.
    ///
    /// # Errors
    ///
    /// Returns an error if Docker Compose fails to start the services.
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), DeployError> {
        info!("Starting Lago services");

        self.run_compose_command(&["up", "-d"]).await?;

        info!("Lago services started successfully");
        Ok(())
    }

    /// Stop the Lago services.
    ///
    /// # Errors
    ///
    /// Returns an error if Docker Compose fails to stop the services.
    #[instrument(skip(self))]
    pub async fn stop(&self) -> Result<(), DeployError> {
        info!("Stopping Lago services");

        self.run_compose_command(&["down"]).await?;

        info!("Lago services stopped successfully");
        Ok(())
    }

    /// Get the status of Lago services.
    ///
    /// # Errors
    ///
    /// Returns an error if Docker Compose fails to get the status.
    #[instrument(skip(self))]
    pub async fn status(&self) -> Result<DeployStatus, DeployError> {
        debug!("Getting Lago service status");

        let output = self.run_compose_command(&["ps", "--format", "json"]).await?;

        let services = parse_compose_ps_output(&output);
        let running = services.iter().any(|(_, s)| *s == ServiceStatus::Running);

        Ok(DeployStatus { running, services })
    }

    /// Get logs from Lago services.
    ///
    /// # Arguments
    ///
    /// * `service` - Optional service name to filter logs. If None, returns logs from all services.
    /// * `tail` - Number of lines to return (default: 100)
    ///
    /// # Errors
    ///
    /// Returns an error if Docker Compose fails to get the logs.
    #[instrument(skip(self))]
    pub async fn logs(&self, service: Option<&str>, tail: Option<u32>) -> Result<String, DeployError> {
        debug!("Getting Lago service logs");

        let tail_arg = format!("--tail={}", tail.unwrap_or(100));
        let mut args = vec!["logs", &tail_arg];

        if let Some(svc) = service {
            args.push(svc);
        }

        self.run_compose_command(&args).await
    }

    /// Restart the Lago services.
    ///
    /// # Errors
    ///
    /// Returns an error if Docker Compose fails to restart the services.
    #[instrument(skip(self))]
    pub async fn restart(&self) -> Result<(), DeployError> {
        info!("Restarting Lago services");

        self.run_compose_command(&["restart"]).await?;

        info!("Lago services restarted successfully");
        Ok(())
    }

    /// Pull the latest Docker images for Lago services.
    ///
    /// # Errors
    ///
    /// Returns an error if Docker Compose fails to pull the images.
    #[instrument(skip(self))]
    pub async fn pull(&self) -> Result<(), DeployError> {
        info!("Pulling latest Lago images");

        self.run_compose_command(&["pull"]).await?;

        info!("Lago images pulled successfully");
        Ok(())
    }

    /// Run a Docker Compose command in the Lago directory.
    async fn run_compose_command(&self, args: &[&str]) -> Result<String, DeployError> {
        let mut command = Command::new("docker");
        command
            .arg("compose")
            .args(args)
            .current_dir(&self.lago_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add environment variables from config if present
        if let Some(ref config) = self.config {
            for (key, value) in config.to_env_vars() {
                command.env(key, value);
            }
        }

        debug!(
            "Running docker compose command: docker compose {}",
            args.join(" ")
        );

        let output = command.output().await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DeployError::DockerNotAvailable("docker command not found".to_string())
            } else {
                DeployError::CommandExecution(e)
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(DeployError::CommandFailed {
                exit_code: output.status.code().unwrap_or(-1),
                stderr,
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// Parse the output of `docker compose ps --format json`.
fn parse_compose_ps_output(output: &str) -> Vec<(String, ServiceStatus)> {
    let mut services = Vec::new();

    // Docker compose ps --format json outputs one JSON object per line
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            let name = json
                .get("Service")
                .or_else(|| json.get("Name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let state = json
                .get("State")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let status = match state.to_lowercase().as_str() {
                "running" => ServiceStatus::Running,
                "exited" | "stopped" | "dead" => ServiceStatus::Stopped,
                "starting" | "created" => ServiceStatus::Starting,
                _ => ServiceStatus::Unknown,
            };

            services.push((name, status));
        }
    }

    services
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_compose_ps_output() {
        let output = r#"{"Service":"api","State":"running"}
{"Service":"db","State":"running"}
{"Service":"redis","State":"exited"}"#;

        let services = parse_compose_ps_output(output);

        assert_eq!(services.len(), 3);
        assert_eq!(services[0], ("api".to_string(), ServiceStatus::Running));
        assert_eq!(services[1], ("db".to_string(), ServiceStatus::Running));
        assert_eq!(services[2], ("redis".to_string(), ServiceStatus::Stopped));
    }

    #[test]
    fn test_parse_empty_output() {
        let services = parse_compose_ps_output("");
        assert!(services.is_empty());
    }
}
