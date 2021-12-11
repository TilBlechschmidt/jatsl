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
use std::{collections::HashMap, convert::Infallible, sync::Arc};

#[derive(Serialize, Eq, PartialEq)]
enum Status {
    Operational,
    Degraded,
    Unrecoverable,
}

/// State of the observed application
///
/// Can be used to manually change the ready-state even though all jobs are healthy.
#[derive(Serialize, Eq, PartialEq, Clone, Copy)]
pub enum State {
    Startup,
    Running,
    Shutdown,
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
    state: State,
    jobs: HashMap<String, String>,
}

/// HTTP healthcheck server
///
/// Makes the job status list available as an HTTP endpoint. Commonly used for Kubernetes or Docker health probes.
/// Note that the server does not automatically schedule itself so you have to start it as a job on a scheduler yourself!
#[derive(Clone)]
pub struct StatusServer {
    status: Arc<Mutex<HashMap<String, JobStatus>>>,
    state: Arc<Mutex<State>>,
    port: u16,
}

impl StatusServer {
    /// Creates a new server for the given scheduler and port configuration
    pub fn new(scheduler: &JobScheduler, port: u16) -> (Arc<Mutex<State>>, Self) {
        let state = Arc::new(Mutex::new(State::Startup));
        (
            state.clone(),
            Self {
                status: scheduler.status.clone(),
                state,
                port,
            },
        )
    }

    async fn generate_report(
        status_map: Arc<Mutex<HashMap<String, JobStatus>>>,
        state: Arc<Mutex<State>>,
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

        let state = state.lock().await;
        if *state != State::Running {
            status = Status::Degraded;
        }

        let status_response = StatusResponse {
            state: *state,
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
impl Job for StatusServer {
    const NAME: &'static str = module_path!();
    const SUPPORTS_GRACEFUL_TERMINATION: bool = true;

    async fn execute(
        &self,
        manager: TaskManager<()>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let status = self.status.clone();
        let state = self.state.clone();

        let make_svc = make_service_fn(|_conn| {
            let status = status.clone();
            let state = state.clone();

            async move {
                Ok::<_, HyperError>(service_fn(move |req| {
                    StatusServer::generate_report(status.clone(), state.clone(), req)
                }))
            }
        });

        let addr = ([0, 0, 0, 0], self.port).into();
        let server = Server::bind(&addr).serve(make_svc);
        let graceful = server.with_graceful_shutdown(manager.termination_signal());

        info!("Status server listening on {}", addr);
        manager.ready().await;
        graceful.await.map_err(anyhow::Error::new)?;

        Ok(())
    }
}
