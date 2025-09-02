use std::{future::Future, pin::pin, time::Duration};

use hyper::{Request, Response, body::Incoming, service::service_fn};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let token = CancellationToken::new();

    let shutdown_token = token.clone();

    tokio::spawn(async move {
        let _gaurd = shutdown_token.clone().drop_guard();
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
    });

    let server = tokio::spawn(http_server(token));

    server.await??;

    Ok(())
}

async fn http_server(token: CancellationToken) -> anyhow::Result<()> {
    println!("starting http server");
    let tracker = TaskTracker::new();

    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    while let Some(Ok((stream, _addr))) = token.run_until_cancelled(listener.accept()).await {
        tracker.spawn(conn_handler(stream, tracker.clone(), token.child_token()));
    }

    println!("shutting down server, waiting for connections to finish");

    tracker.close();
    tracker.wait().await;

    Ok(())
}

async fn conn_handler(
    stream: TcpStream,
    tracker: TaskTracker,
    token: CancellationToken,
) -> anyhow::Result<()> {
    println!("handling connection");
    let builder = hyper_util::server::conn::auto::Builder::new(TaskExecutor(tracker));
    let mut conn = pin!(builder.serve_connection(
        TokioIo::new(stream),
        service_fn(|req| async { req_handler(req).await }),
    ));

    match token.run_until_cancelled(conn.as_mut()).await {
        Some(res) => return res.map_err(|e| anyhow::anyhow!(e)),
        None => {
            println!("connection cancelled; Completing any outstanding requests");
            conn.as_mut().graceful_shutdown();
            let res = conn.await;
            return res.map_err(|e| anyhow::anyhow!(e));
        }
    }
}

async fn req_handler(req: Request<Incoming>) -> anyhow::Result<Response<String>> {
    println!("received http request at {}", req.uri());

    tokio::time::sleep(Duration::from_secs(5)).await;

    anyhow::Ok(Response::new("hello world\n".to_string()))
}

#[derive(Clone)]
struct TaskExecutor(TaskTracker);

impl<Fut> hyper::rt::Executor<Fut> for TaskExecutor
where
    Fut: Future + Send + 'static,
    Fut::Output: Send,
{
    fn execute(&self, fut: Fut) {
        // This isn't really necessary (i.e. using the task tracker); hyper tracks internally.
        self.0.spawn(fut);
    }
}
