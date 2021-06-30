use anyhow::Result;
use async_trait::async_trait;
use jatsl::{Job, JobManager, JobScheduler};
use log::warn;

#[derive(Clone)]
struct ExampleContext {
    name: &'static str,
}

struct ExampleJob {
    name: &'static str,
}

#[async_trait]
impl Job for ExampleJob {
    const NAME: &'static str = "ExampleJob";
    const SUPPORTS_GRACEFUL_TERMINATION: bool = true;

    async fn execute(
        &self,
        manager: JobManager,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Do some startup work (e.g. connecting to databases)
        warn!("Hello {}!", self.name);

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
    let job = ExampleJob { name: "world" };

    scheduler.spawn_job(job);

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    scheduler.terminate_jobs().await;
}
