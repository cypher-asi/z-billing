# Z-Billing v0.1.0 - Lago Deployment

This document specifies the `z-billing-lago` crate for managing Lago deployment.

## Overview

The `z-billing-lago` crate provides Docker Compose management for Lago:

- Start, stop, restart Lago services
- Check service status
- Retrieve service logs
- Configure deployment via environment variables

## Crate Structure

```
z-billing-lago/
├── Cargo.toml
└── src/
    ├── lib.rs       # Public exports
    ├── deploy.rs    # LagoDeployment, ServiceStatus
    ├── config.rs    # LagoConfig
    └── error.rs     # DeployError
```

## LagoDeployment

Manages Lago deployment via Docker Compose.

### Structure

```rust
pub struct LagoDeployment {
    /// Path to the cloned Lago repository.
    lago_path: PathBuf,
    /// Optional configuration overrides.
    config: Option<LagoConfig>,
}
```

### Constructor

```rust
impl LagoDeployment {
    /// Create a new deployment manager.
    ///
    /// # Arguments
    /// * `lago_path` - Path to cloned Lago repository containing docker-compose.yml
    ///
    /// # Errors
    /// * `PathNotFound` - Path does not exist
    /// * `ComposeFileNotFound` - No docker-compose.yml in directory
    pub fn new(lago_path: impl AsRef<Path>) -> Result<Self, DeployError>;

    /// Set configuration for the deployment.
    pub fn with_config(mut self, config: LagoConfig) -> Self;
}
```

### Operations

#### start()

Start all Lago services in detached mode.

```rust
deployment.start().await?;
```

Runs: `docker compose up -d`

#### stop()

Stop all Lago services.

```rust
deployment.stop().await?;
```

Runs: `docker compose down`

#### restart()

Restart all Lago services.

```rust
deployment.restart().await?;
```

Runs: `docker compose restart`

#### pull()

Pull latest Docker images for Lago services.

```rust
deployment.pull().await?;
```

Runs: `docker compose pull`

#### status()

Get the current status of all services.

```rust
let status = deployment.status().await?;
println!("Running: {}", status.running);
for (name, state) in &status.services {
    println!("  {}: {:?}", name, state);
}
```

Runs: `docker compose ps --format json`

#### logs()

Get logs from services.

```rust
// All services, last 100 lines
let logs = deployment.logs(None, None).await?;

// Specific service, last 50 lines
let api_logs = deployment.logs(Some("api"), Some(50)).await?;
```

Runs: `docker compose logs --tail=<n> [service]`

## ServiceStatus

Status of a Docker Compose service.

```rust
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
```

### State Mapping

| Docker State    | ServiceStatus  |
|-----------------|----------------|
| `running`       | `Running`      |
| `exited`        | `Stopped`      |
| `stopped`       | `Stopped`      |
| `dead`          | `Stopped`      |
| `starting`      | `Starting`     |
| `created`       | `Starting`     |
| (other)         | `Unknown`      |

## DeployStatus

Overall deployment status.

```rust
pub struct DeployStatus {
    /// Whether any service is running.
    pub running: bool,
    /// Individual service statuses.
    pub services: Vec<(String, ServiceStatus)>,
}
```

## LagoConfig

Configuration for Lago deployment.

### Structure

```rust
pub struct LagoConfig {
    /// The API URL for Lago (default: http://localhost:3000)
    pub api_url: String,

    /// The front-end URL for Lago (default: http://localhost:80)
    pub front_url: String,

    /// PostgreSQL database URL
    pub database_url: String,

    /// Redis URL for caching and jobs
    pub redis_url: String,

    /// Secret key base for Rails encryption
    pub secret_key_base: String,

    /// RSA private key for JWT signing (optional)
    pub rsa_private_key: Option<String>,

    /// Lago API key for authentication
    pub api_key: Option<String>,

    /// Enable or disable signup (default: true)
    pub disable_signup: bool,
}
```

### Default Values

```rust
LagoConfig {
    api_url: "http://localhost:3000",
    front_url: "http://localhost:80",
    database_url: "postgresql://lago:lago@localhost:5432/lago",
    redis_url: "redis://localhost:6379",
    secret_key_base: /* generated */,
    rsa_private_key: None,
    api_key: None,
    disable_signup: false,
}
```

### Builder Pattern

```rust
let config = LagoConfig::new()
    .with_api_url("http://api.lago.local:3000")
    .with_front_url("http://lago.local")
    .with_database_url("postgresql://user:pass@db:5432/lago")
    .with_redis_url("redis://redis:6379")
    .with_api_key("my-secret-api-key");
```

### Environment Variable Mapping

```rust
impl LagoConfig {
    pub fn to_env_vars(&self) -> Vec<(String, String)>;
}
```

| Config Field       | Environment Variable     |
|--------------------|--------------------------|
| `api_url`          | `LAGO_API_URL`           |
| `front_url`        | `LAGO_FRONT_URL`         |
| `database_url`     | `DATABASE_URL`           |
| `redis_url`        | `REDIS_URL`              |
| `secret_key_base`  | `SECRET_KEY_BASE`        |
| `rsa_private_key`  | `LAGO_RSA_PRIVATE_KEY`   |
| `api_key`          | `LAGO_API_KEY`           |
| `disable_signup`   | `LAGO_DISABLE_SIGNUP`    |

## DeployError

Errors that can occur during deployment operations.

```rust
pub enum DeployError {
    /// The Lago repository path does not exist.
    PathNotFound(PathBuf),

    /// docker-compose.yml not found in directory.
    ComposeFileNotFound(PathBuf),

    /// Failed to execute docker compose command.
    CommandExecution(std::io::Error),

    /// Docker compose command returned non-zero exit code.
    CommandFailed { exit_code: i32, stderr: String },

    /// Failed to parse command output.
    ParseError(String),

    /// Docker is not installed or not running.
    DockerNotAvailable(String),
}
```

## Usage Example

```rust
use z_billing_lago::{LagoDeployment, LagoConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create configuration
    let config = LagoConfig::new()
        .with_api_key("my-api-key")
        .with_database_url("postgresql://lago:lago@db:5432/lago");

    // Create deployment manager
    let deployment = LagoDeployment::new("./lago")?
        .with_config(config);

    // Start services
    println!("Starting Lago...");
    deployment.start().await?;

    // Check status
    let status = deployment.status().await?;
    println!("Lago running: {}", status.running);

    // Get API logs
    let logs = deployment.logs(Some("api"), Some(50)).await?;
    println!("API logs:\n{}", logs);

    // Stop when done
    deployment.stop().await?;

    Ok(())
}
```

## Deployment Lifecycle Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      Lago Deployment Lifecycle                              │
└─────────────────────────────────────────────────────────────────────────────┘

                         ┌───────────────┐
                         │   Stopped     │
                         │  (no services)│
                         └───────┬───────┘
                                 │
                    start()      │
                                 ▼
                         ┌───────────────┐
                         │   Starting    │
                         │  (pulling     │
                         │   images,     │
                         │   creating    │
                         │   containers) │
                         └───────┬───────┘
                                 │
                                 ▼
        ┌────────────────┬───────────────┬────────────────┐
        │                │               │                │
        ▼                ▼               ▼                ▼
   ┌─────────┐     ┌─────────┐    ┌─────────┐     ┌─────────┐
   │   API   │     │   DB    │    │  Redis  │     │  Worker │
   │ Running │     │ Running │    │ Running │     │ Running │
   └────┬────┘     └────┬────┘    └────┬────┘     └────┬────┘
        │               │              │               │
        └───────────────┴──────┬───────┴───────────────┘
                               │
                        status()
                               │
                               ▼
                      ┌────────────────┐
                      │  DeployStatus  │
                      │  running: true │
                      │  services: [   │
                      │    (api, Run)  │
                      │    (db, Run)   │
                      │    ...         │
                      │  ]             │
                      └────────────────┘
                               │
              ┌────────────────┼────────────────┐
              │                │                │
         restart()         stop()          logs()
              │                │                │
              ▼                ▼                ▼
        ┌───────────┐   ┌───────────┐   ┌───────────┐
        │ Restart   │   │ Stopped   │   │ Return    │
        │ all       │   │ (down)    │   │ log       │
        │ services  │   │           │   │ output    │
        └───────────┘   └───────────┘   └───────────┘
```

## Lago Services

Lago Docker Compose typically includes:

| Service     | Description                    | Port      |
|-------------|--------------------------------|-----------|
| `api`       | Lago Rails API server          | 3000      |
| `front`     | Lago frontend (React)          | 80        |
| `db`        | PostgreSQL database            | 5432      |
| `redis`     | Redis for caching/jobs         | 6379      |
| `worker`    | Sidekiq background worker      | -         |
| `clock`     | Scheduled job runner           | -         |
| `pdf`       | PDF generation service         | -         |

## Prerequisites

- Docker and Docker Compose installed
- Lago repository cloned locally
- Sufficient disk space for Docker images

## Command Execution

All commands are executed via `docker compose` with:

1. Working directory set to `lago_path`
2. Environment variables from `LagoConfig` (if provided)
3. Stdout/stderr captured for parsing and error reporting

```rust
async fn run_compose_command(&self, args: &[&str]) -> Result<String, DeployError> {
    let mut command = Command::new("docker");
    command
        .arg("compose")
        .args(args)
        .current_dir(&self.lago_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Add environment variables from config
    if let Some(ref config) = self.config {
        for (key, value) in config.to_env_vars() {
            command.env(key, value);
        }
    }

    // Execute and handle result
    // ...
}
```
