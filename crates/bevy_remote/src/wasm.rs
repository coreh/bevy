use std::sync::{OnceLock, RwLock};

use bevy_app::{App, Plugin};
use bevy_ecs::system::NonSendMut;
use bevy_log::error;
use bevy_utils::HashMap;
use serde::Serialize;
use serde_wasm_bindgen::{from_value, Serializer};
use wasm_bindgen::prelude::*;

use crate::{BrpId, BrpRequest, BrpResponseContent, Remote, RemoteSession, RemoteSessions};

static WASM_REMOTE_SESSION: OnceLock<RemoteSession> = OnceLock::new();

thread_local! {
    static REQUEST_PROMISE_CALLBACKS: RwLock<HashMap<BrpId, (js_sys::Function, js_sys::Function)>> = RwLock::new(HashMap::default());
}

#[wasm_bindgen(js_name = "sendRequest")]
pub fn send_request(brp_request: JsValue) -> Result<JsValue, JsValue> {
    Ok(js_sys::Promise::new(
        &mut (move |resolve, reject| {
            let reject_or_log = |err: JsError| {
                if let Err(err) = reject.call1(&JsValue::undefined(), &err.into()) {
                    error!("Failed to call reject callback: {:?}", err);
                }
            };

            let brp_request = match from_value::<BrpRequest>(brp_request.clone()) {
                Err(err) => {
                    reject_or_log(JsError::new(
                        format!("Failed to deserialize BRP request: {}", err).as_str(),
                    ));
                    return;
                }
                Ok(brp_request) => brp_request,
            };

            let id = brp_request.id;

            let Some(session) = WASM_REMOTE_SESSION.get() else {
                reject_or_log(JsError::new("WASM session not initialized").into());
                return;
            };

            if let Err(err) = session.request_sender.send(brp_request) {
                reject_or_log(
                    JsError::new(format!("Failed to send BRP request: {}", err).as_str()).into(),
                );
                return;
            }

            REQUEST_PROMISE_CALLBACKS.with(|callbacks| {
                let mut callbacks = callbacks.write().unwrap();
                callbacks.insert(id, (resolve, reject));
            });
        }),
    )
    .into())
}

pub struct WasmRemotePlugin;

impl Plugin for WasmRemotePlugin {
    fn build(&self, app: &mut App) {
        WASM_REMOTE_SESSION.get_or_init(|| {
            app.world.insert_non_send_resource(());
            let brp_sessions = app.world.get_resource::<RemoteSessions>().unwrap();
            brp_sessions.open("WASM", crate::RemoteComponentFormat::Json)
        });

        app.add_systems(Remote, process_brp_responses);
    }
}

fn process_brp_responses(_: NonSendMut<()>) {
    REQUEST_PROMISE_CALLBACKS.with(|callbacks| {
        let serializer = Serializer::json_compatible();
        let mut callbacks = callbacks.write().unwrap();
        let session = WASM_REMOTE_SESSION.get().unwrap();
        while let Ok(response) = session.response_receiver.try_recv() {
            if let Some((resolve, reject)) = callbacks.remove(&response.id) {
                let reject_or_log = |err: JsError| {
                    if let Err(err) = reject.call1(&JsValue::undefined(), &err.into()) {
                        error!("Failed to call reject callback: {:?}", err);
                    }
                };

                let resolve_or_log = |value: JsValue| {
                    if let Err(err) = resolve.call1(&JsValue::undefined(), &value) {
                        error!("Failed to call resolve callback: {:?}", err);
                    }
                };

                match response.response {
                    BrpResponseContent::Error(err) => {
                        reject_or_log(JsError::new(format!("BRP error: {:?}", err).as_str()));
                    }
                    _ => {
                        resolve_or_log(response.serialize(&serializer).unwrap());
                    }
                }
            }
        }
    });
}
