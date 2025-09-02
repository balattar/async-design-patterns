use anyhow::Context;
use std::future::Future;
use tokio::sync::mpsc;

pub trait Actor: Send {
    type Req: Sync + Send + 'static;
    type Reply: Sync + Send + 'static;

    fn handle(&mut self, msg: Self::Req) -> impl Future<Output = Self::Reply> + Send;
}

// TODO: vvvvvv

pub fn actor_spawn<A: Actor + 'static>(actor: A) -> MailboxRef<A> {
    let (tx, rx) = mpsc::unbounded_channel::<(A::Req, tokio::sync::oneshot::Sender<A::Reply>)>();

    let mailbox = MailboxRef(tx);
    tokio::spawn(async move {
        let mut rx = rx;
        let mut actor = actor;

        while let Some((req, resp_tx)) = rx.recv().await {
            let res = actor.handle(req).await;
            let _ = resp_tx.send(res);
        }
    });

    mailbox
}

// #[derive(Debug, Clone)]
pub struct MailboxRef<A: Actor>(
    tokio::sync::mpsc::UnboundedSender<(A::Req, tokio::sync::oneshot::Sender<A::Reply>)>,
);

impl<A: Actor> Clone for MailboxRef<A> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<A: Actor> MailboxRef<A> {
    pub async fn ask(&self, req: A::Req) -> anyhow::Result<A::Reply> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.0.send((req, tx)).context("Actor is closed")?;

        rx.await
            .map_err(|e| anyhow::anyhow!("Actor task is gone: {}", e))
    }
}
