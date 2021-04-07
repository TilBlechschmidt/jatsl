use anyhow::Result;
use async_trait::async_trait;
use jatsl::{Job, JobScheduler, TaskManager};
use log::warn;

#[derive(Clone)]
struct ExampleContext {
    name: &'static str,
}

struct ExampleJob;

#[async_trait]
impl Job for ExampleJob {
    type Context = ExampleContext;

    const NAME: &'static str = "ExampleJob";
    const SUPPORTS_GRACEFUL_TERMINATION: bool = true;

    async fn execute(&self, manager: TaskManager<Self::Context>) -> Result<()> {
        // Do some startup work (e.g. connecting to databases)
        warn!("Hello {}!", manager.context.name);

        // Signal that we have completed startup and are ready to fulfill our job (pun intended)
        manager.ready().await;

        // Do some cool work, serve some requests, be awesome :)
        //
        // Terminate nicely when we are asked to (useful for e.g. HTTP servers).
        // Leaving this out and just running in an infinite loop is also fine, the job will just be killed.
        //      (make sure to set SUPPORTS_GRACEFUL_TERMINATION to false [or just remove it as it is the default])
        manager.termination_signal().await;

        Ok(())
    }
}

#[tokio::main]
async fn main() {
    std::env::set_var("RUST_LOG", "info");
    pretty_env_logger::init();

    let scheduler = JobScheduler::default();
    let ctx = ExampleContext { name: "world" };
    let job = ExampleJob {};

    scheduler.spawn_job(job, ctx);

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    scheduler.terminate_jobs().await;
}
