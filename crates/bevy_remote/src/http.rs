use crate::{brp::*, BrpChannels};
use bevy_app::{App, Plugin};
use std::time::Duration;

pub struct HttpRemotePlugin;

const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_millis(500);

impl From<BrpResponse> for rouille::Response {
    fn from(brp_response: BrpResponse) -> Self {
        let response = Self::json(&brp_response);

        let response = match brp_response.response {
            BrpResponseContent::Error(err) => match err {
                BrpError::EntityNotFound => response.with_status_code(404),
                BrpError::ComponentNotFound => response.with_status_code(404),
                BrpError::Timeout => response.with_status_code(408),
                BrpError::InternalError => response.with_status_code(500),
                _ => response.with_status_code(400),
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
        let brp_channels = app.world.get_resource::<BrpChannels>().unwrap();
        let request_sender = brp_channels.request_sender.clone();
        let response_receiver = brp_channels.response_receiver.clone();
        let response_loopback = brp_channels.response_sender.clone();

        // spawn the http thread
        std::thread::spawn(move || {
            rouille::start_server("localhost:8765", move |request| {
                let Ok(brp_request) = BrpRequest::try_from(request) else {
                    return BrpResponse::from_error(0, BrpError::InvalidRequest).into();
                };

                let now = std::time::Instant::now();

                let id = brp_request.id;

                match request_sender.send(brp_request) {
                    Ok(_) => {}
                    Err(_) => {
                        return BrpResponse::from_error(id, BrpError::InternalError).into();
                    }
                };

                loop {
                    match response_receiver.recv() {
                        Ok(brp_response) => {
                            if brp_response.id == id {
                                // The response is for this request
                                return brp_response.into();
                            } else {
                                if now.elapsed() > HTTP_REQUEST_TIMEOUT {
                                    return BrpResponse::from_error(id, BrpError::Timeout).into();
                                }

                                // The response is not for this request, so send it back to the loopback
                                // This is a hack to avoid having to implement a hashmap of request ids
                                response_loopback.send(brp_response).unwrap();
                            }
                        }
                        Err(_) => {
                            return BrpResponse::from_error(id, BrpError::InternalError).into();
                        }
                    }
                }
            });
        });
    }
}
