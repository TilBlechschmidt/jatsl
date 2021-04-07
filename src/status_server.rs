use super::{Job, JobScheduler, JobStatus, TaskManager};
use anyhow::Result;
use async_trait::async_trait;
use futures::lock::Mutex;
use hyper::{
    header::CONTENT_TYPE,
    server::Server,
    service::{make_service_fn, service_fn},
    Body, Error as HyperError, Request, Response, StatusCode,
};
use log::info;
use serde::Serialize;
use serde_json::to_string;
use std::{collections::HashMap, convert::Infallible, marker::PhantomData, sync::Arc};

#[derive(Serialize, Eq, PartialEq)]
enum Status {
    Operational,
    Degraded,
    Unrecoverable,
}

impl Status {
    fn status_code(&self) -> StatusCode {
        match *self {
            Status::Operational => StatusCode::OK,
            Status::Degraded => StatusCode::SERVICE_UNAVAILABLE,
            Status::Unrecoverable => StatusCode::GONE,
        }
    }
}

#[derive(Serialize)]
struct StatusResponse<'a> {
    status: &'a Status,
    jobs: HashMap<String, String>,
}

/// HTTP healthcheck server
///
/// Makes the job status list available as an HTTP endpoint. Commonly used for Kubernetes or Docker health probes.
/// Note that the server does not automatically schedule itself so you have to start it as a job on a scheduler yourself!
#[derive(Clone)]
pub struct StatusServer<C> {
    status: Arc<Mutex<HashMap<String, JobStatus>>>,
    config: Option<Option<u16>>,
    phantom: PhantomData<C>,
}

impl<C> StatusServer<C> {
    /// Creates a new server for the given scheduler and port configuration
    ///
    /// If the config is `None` the server is disabled. If it is Some(None), the default port is used. Otherwise the provided port is used.
    pub fn new(scheduler: &JobScheduler, config: Option<Option<u16>>) -> Self {
        Self {
            status: scheduler.status.clone(),
            config,
            phantom: PhantomData,
        }
    }

    async fn generate_report(
        status_map: Arc<Mutex<HashMap<String, JobStatus>>>,
        _req: Request<Body>,
    ) -> Result<Response<Body>, Infallible> {
        let status_map = status_map.lock().await;
        let mut status = Status::Operational;
        let mut jobs = HashMap::new();

        for (job_name, job_status) in status_map.iter() {
            match *job_status {
                JobStatus::Terminated => status = Status::Unrecoverable,
                JobStatus::Restarting | JobStatus::CrashLoopBackOff | JobStatus::Startup => {
                    if status != Status::Unrecoverable {
                        status = Status::Degraded
                    }
                }
                _ => {}
            };

            jobs.insert(job_name.clone(), format!("{}", job_status));
        }

        let status_response = StatusResponse {
            status: &status,
            jobs,
        };

        let body = to_string(&status_response).unwrap();

        let response = Response::builder()
            .status(status.status_code())
            .header(CONTENT_TYPE, "application/json")
            .body(body.into());

        Ok(response.unwrap())
    }
}

#[async_trait]
impl<C: Send + Sync + 'static> Job for StatusServer<C> {
    type Context = C;

    const NAME: &'static str = module_path!();
    const SUPPORTS_GRACEFUL_TERMINATION: bool = true;

    async fn execute(&self, manager: TaskManager<Self::Context>) -> Result<()> {
        if let Some(port_config) = self.config {
            let port = port_config.unwrap_or(47002);

            let status = self.status.clone();
            let make_svc = make_service_fn(|_conn| {
                let status = status.clone();

                async move {
                    Ok::<_, HyperError>(service_fn(move |req| {
                        StatusServer::<C>::generate_report(status.clone(), req)
                    }))
                }
            });

            let addr = ([0, 0, 0, 0], port).into();
            let server = Server::bind(&addr).serve(make_svc);
            let graceful = server.with_graceful_shutdown(manager.termination_signal());

            info!("Status server listening on {}", addr);
            manager.ready().await;
            graceful.await?;

            Ok(())
        } else {
            info!("Status server is disabled, exiting.");
            Ok(())
        }
    }
}