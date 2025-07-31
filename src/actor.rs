use crate::shutdown::{ShutdownCoordinator, ShutdownHook};
use async_trait::async_trait;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tracing::error;

// https://ryhl.io/blog/actors-with-tokio/
#[async_trait]
pub trait Actor<T: Send + Sync>: ShutdownHook {
    async fn handle_msg(&mut self, msg: T);
    fn receiver(&mut self) -> &mut Receiver<T>;
}

#[derive(Clone)]
pub struct ActorHandle<T: Clone> {
    sender: Sender<T>,
}

impl<T: Clone + Send + Sync + 'static> ActorHandle<T> {
    pub fn spawn(
        mk_actor: impl Fn(Receiver<T>, ActorHandle<T>) -> Box<dyn Actor<T> + Send + Sync>,
        shutdown: &mut ShutdownCoordinator,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(8);
        let handle = Self { sender };
        let mut actor = mk_actor(receiver, handle.clone());
        let completion = shutdown.token();
        let jhandle = tokio::spawn(async move {
            tokio::select! {
              _ = run_actor(&mut actor) => {}
              _ = completion.cancelled() => {
                if let Err(e) = actor.shutdown().await {
                  error!("Graceful shutdown failed for actor. {}", e);
                }
              }
            }
        });
        shutdown.register_task(jhandle);
        handle
    }

    pub async fn send(&self, msg: T) {
        let _ = self.sender.send(msg).await;
    }
}

async fn run_actor<T: Send + Sync>(actor: &mut Box<dyn Actor<T> + Send + Sync>) {
    while let Some(msg) = actor.receiver().recv().await {
        actor.handle_msg(msg).await
    }
}
