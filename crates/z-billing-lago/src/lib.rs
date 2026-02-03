//! Lago deployment and management for z-billing.
//!
//! This crate provides tooling to manage Lago deployment via Docker Compose,
//! including starting, stopping, and monitoring the Lago services.
//!
//! # Example
//!
//! ```no_run
//! use z_billing_lago::{LagoDeployment, LagoConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a deployment manager pointing to the cloned Lago repo
//! let deployment = LagoDeployment::new("./lago")?;
//!
//! // Start Lago services
//! deployment.start().await?;
//!
//! // Check status
//! let status = deployment.status().await?;
//! println!("Lago running: {}", status.running);
//!
//! // Get logs
//! let logs = deployment.logs(Some("api"), None).await?;
//! println!("API logs: {}", logs);
//!
//! // Stop services
//! deployment.stop().await?;
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

mod config;
mod deploy;
mod error;

pub use config::LagoConfig;
pub use deploy::{DeployStatus, LagoDeployment, ServiceStatus};
pub use error::DeployError;
