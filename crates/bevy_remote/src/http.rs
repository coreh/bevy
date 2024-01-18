use crate::{brp::*, RemoteSessions};
use bevy_app::{App, Plugin};
use bevy_log::{debug, warn};
use std::time::Duration;

pub struct HttpRemotePlugin;

const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_millis(500);

impl From<BrpResponse> for rouille::Response {
    fn from(brp_response: BrpResponse) -> Self {
        let response = Self::json(&brp_response);

        let response = match brp_response.response {
            BrpResponseContent::Error(err) => match err {
                BrpError::EntityNotFound => response.with_status_code(404), // Not found
                BrpError::ComponentNotFound(_) => response.with_status_code(404), // Not found
                BrpError::Timeout => response.with_status_code(408),        // Request timeout
                BrpError::InternalError => response.with_status_code(500),  // Internal server error
                BrpError::Unimplemented => response.with_status_code(501),  // Not implemented
                BrpError::ComponentAmbiguous(_) => response.with_status_code(409), // Conflict
                BrpError::ComponentMissingTypeId(_) => response.with_status_code(415), // Unsupported media type
                BrpError::ComponentMissingTypeRegistration(_) => response.with_status_code(415), // Unsupported media type
                BrpError::ComponentMissingReflect(_) => response.with_status_code(415), // Unsupported media type
                BrpError::ComponentInvalidAccess(_) => response.with_status_code(423),  // Locked
                _ => response.with_status_code(400), // Bad request
            },
            _ => response,
        };

        return response;
    }
}

impl TryFrom<&rouille::Request> for BrpRequest {
    type Error = rouille::input::json::JsonError;

    fn try_from(request: &rouille::Request) -> Result<Self, Self::Error> {
        rouille::input::json_input(request)
    }
}

impl Plugin for HttpRemotePlugin {
    fn build(&self, app: &mut App) {
        let brp_sessions = app.world.get_resource::<RemoteSessions>().unwrap();
        let brp_session = brp_sessions.open("HTTP", crate::RemoteComponentFormat::Json);
        let request_sender = brp_session.request_sender.clone();
        let response_receiver = brp_session.response_receiver.clone();
        let response_loopback = brp_session.response_sender.clone();

        // atomic counter for request ids
        let request_id = std::sync::atomic::AtomicU64::new(0);

        // spawn the http thread
        std::thread::spawn(move || {
            rouille::start_server(
                "localhost:8765",
                with_cors(move |request| {
                    if request.url() == "/" && request.method() == "GET" {
                        return rouille::Response::html(include_str!("index.html"));
                    }

                    if request.url() != "/brp" {
                        warn!("Invalid URL: {}", request.url());
                        return rouille::Response::empty_404();
                    }

                    if request.method() != "POST" {
                        warn!("Invalid HTTP method: {}", request.method());
                        return BrpResponse::from_error(0, BrpError::InvalidRequest).into();
                    }

                    let Ok(mut brp_request) = BrpRequest::try_from(request) else {
                        warn!("Invalid request: {:?}", request);
                        return BrpResponse::from_error(0, BrpError::InvalidRequest).into();
                    };

                    // For HTTP, ignore the request id from the client and generate a new one
                    brp_request.id = request_id
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                        .into();

                    let now = std::time::Instant::now();

                    let id = brp_request.id;

                    debug!("Sending request to channel: {:?}", brp_request);

                    match request_sender.send(brp_request) {
                        Ok(_) => {}
                        Err(_) => {
                            warn!("Failed to send request to channel");
                            return BrpResponse::from_error(id, BrpError::InternalError).into();
                        }
                    };

                    let deadline = now + HTTP_REQUEST_TIMEOUT;

                    loop {
                        match response_receiver.recv_deadline(deadline) {
                            Ok(brp_response) => {
                                debug!("Received response from channel: {:?}", brp_response);
                                if brp_response.id == id {
                                    // The response is for this request
                                    return brp_response.into();
                                } else {
                                    if now.elapsed() > HTTP_REQUEST_TIMEOUT {
                                        warn!("Request timed out");
                                        return BrpResponse::from_error(id, BrpError::Timeout)
                                            .into();
                                    }

                                    // The response is not for this request, so send it back to the loopback
                                    // This is a hack to avoid having to implement a hashmap of request ids
                                    debug!("Sending response to loopback: {:?}", brp_response);
                                    response_loopback.send(brp_response).unwrap();
                                }
                            }
                            Err(err) => {
                                if err == crossbeam_channel::RecvTimeoutError::Timeout {
                                    warn!("Request timed out");
                                    return BrpResponse::from_error(id, BrpError::Timeout).into();
                                }

                                warn!("Failed to receive response from channel");
                                return BrpResponse::from_error(id, BrpError::InternalError).into();
                            }
                        }
                    }
                }),
            );
        });

        fn with_cors(
            handler: impl Fn(&rouille::Request) -> rouille::Response,
        ) -> impl Fn(&rouille::Request) -> rouille::Response {
            move |request| {
                if request.method() == "OPTIONS" {
                    rouille::Response::empty_204()
                } else {
                    handler(request)
                }
                .with_additional_header("Access-Control-Allow-Origin", "*")
                .with_additional_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
                .with_additional_header("Access-Control-Allow-Headers", "Content-Type")
            }
        }
    }
}
