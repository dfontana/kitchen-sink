use anyhow::Result;
use async_trait::async_trait;
use futures::future;
use tokio::{
    signal::{
        self,
        unix::{self, SignalKind},
    },
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, instrument};

#[async_trait]
pub trait ShutdownHook: Send + Sync {
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

pub struct ShutdownCoordinator {
    token: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            tasks: Vec::new(),
        }
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    pub fn register_task(&mut self, task: JoinHandle<()>) {
        self.tasks.push(task);
    }

    #[instrument(name = "shutdown", level = "INFO", skip(self))]
    pub async fn wait_for_shutdown(self) {
        info!("Waiting for shutdown signals");
        let mut sigint =
            unix::signal(SignalKind::interrupt()).expect("Failed to install SIGINT handler");
        let mut sigterm =
            unix::signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler");

        tokio::select! {
            _ = sigint.recv() => {info!("Received SIGINT")}
            _ = sigterm.recv() => {info!("Received SIGTERM")}
            _ = signal::ctrl_c() => {info!("Received Ctrl+C")}
            _ = self.token.cancelled() => {info!("Shutdown requested programmatically")}
        }

        info!("Starting shutdown sequence");
        self.token.cancel();
        for res in future::join_all(self.tasks).await {
            if let Err(e) = res {
                error!("Shutdown hook failed: {}", e);
            }
        }
        info!("Shutdown sequence complete");
    }
}
