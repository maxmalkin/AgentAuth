//! Service process spawner for stability tests.
//!
//! Spawns registry/verifier binaries as child processes and waits for health.

use std::time::Duration;

/// A running service process.
#[allow(dead_code)] // Used by stability tests that are #[ignore]
pub struct ServiceProcess {
    child: tokio::process::Child,
    /// The base URL of the running service.
    pub base_url: String,
}

#[allow(dead_code)] // Used by stability tests that are #[ignore]
impl ServiceProcess {
    /// Spawn the registry binary on the given port.
    pub async fn spawn_registry(port: u16, metrics_port: u16) -> Self {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://agentauth:agentauth@localhost:5434/agentauth".into());
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6399".into());

        let child = tokio::process::Command::new("cargo")
            .args(["run", "--bin", "registry", "--"])
            .env("AGENTAUTH__SERVER__PORT", port.to_string())
            .env("AGENTAUTH__SERVER__METRICS_PORT", metrics_port.to_string())
            .env("AGENTAUTH__SERVER__HOST", "127.0.0.1")
            .env("AGENTAUTH__DATABASE__PRIMARY_URL", &db_url)
            .env("AGENTAUTH__REDIS__URLS", &redis_url)
            .env("AGENTAUTH__KMS__BACKEND", "encrypted_keyfile")
            .env("AGENTAUTH__KMS__SIGNING_KEY_ID", "test-stability-key")
            .env("AGENTAUTH__OBSERVABILITY__LOG_LEVEL", "warn")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("failed to spawn registry");

        let base_url = format!("http://127.0.0.1:{port}");

        let proc = Self { child, base_url };
        proc.wait_healthy(Duration::from_secs(30)).await;
        proc
    }

    /// Spawn the verifier binary on the given port.
    pub async fn spawn_verifier(port: u16, metrics_port: u16) -> Self {
        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://agentauth:agentauth@localhost:5434/agentauth".into());
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6399".into());

        let child = tokio::process::Command::new("cargo")
            .args(["run", "--bin", "verifier", "--"])
            .env("AGENTAUTH_VERIFIER__SERVER__PORT", port.to_string())
            .env(
                "AGENTAUTH_VERIFIER__SERVER__METRICS_PORT",
                metrics_port.to_string(),
            )
            .env("AGENTAUTH_VERIFIER__SERVER__HOST", "127.0.0.1")
            .env("AGENTAUTH_VERIFIER__DATABASE__PRIMARY_URL", &db_url)
            .env("AGENTAUTH_VERIFIER__REDIS__URLS", &redis_url)
            .env("AGENTAUTH_VERIFIER__VERIFICATION__REQUIRE_DPOP", "false")
            .env("AGENTAUTH_VERIFIER__OBSERVABILITY__LOG_LEVEL", "warn")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("failed to spawn verifier");

        let base_url = format!("http://127.0.0.1:{port}");

        let proc = Self { child, base_url };
        proc.wait_healthy(Duration::from_secs(30)).await;
        proc
    }

    /// Wait for the service health endpoint to return 200.
    async fn wait_healthy(&self, timeout: Duration) {
        let client = reqwest::Client::new();
        let url = format!("{}/health/live", self.base_url);
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        panic!(
            "service at {} did not become healthy within {:?}",
            self.base_url, timeout
        );
    }

    /// Kill the service process.
    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }
}

impl Drop for ServiceProcess {
    fn drop(&mut self) {
        // Best-effort kill; async kill happens in tests via kill() method
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.child.start_kill();
    }
}
