use crate::JobManager;
use async_trait::async_trait;

/// Persistent execution unit
///
/// Jobs can be dependent on resource (which can be basically anything you define).
/// If such a resource dependency becomes unavailable the job is terminated and restarted.
///
/// In addition, jobs can support graceful shutdown and a ready state provided by the TaskManager passed to the execute function.
#[async_trait]
pub trait Job {
    /// Name of the job displayed in log messages
    const NAME: &'static str;
    /// Whether or not the job honors the termination signal. When this is set to false the job will be terminated externally.
    const SUPPORTS_GRACEFUL_TERMINATION: bool = false;

    fn name(&self) -> String {
        Self::NAME.to_owned()
    }

    fn supports_graceful_termination(&self) -> bool {
        Self::SUPPORTS_GRACEFUL_TERMINATION
    }

    async fn execute(
        &self,
        manager: JobManager,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
}
