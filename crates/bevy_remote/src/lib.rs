use bevy_app::{App, Plugin};
use bevy_ecs::system::Resource;
use brp::*;
use crossbeam_channel::{Receiver, Sender};

pub mod brp;

#[cfg(feature = "http")]
pub mod http;

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        let (request_sender, request_receiver) = crossbeam_channel::unbounded::<BrpRequest>();
        let (response_sender, response_receiver) = crossbeam_channel::unbounded::<BrpResponse>();

        app.insert_resource(BrpChannels {
            request_sender,
            request_receiver,
            response_sender,
            response_receiver,
        });

        #[cfg(feature = "http")]
        app.add_plugins(http::HttpRemotePlugin);
    }
}

#[derive(Resource)]
pub struct BrpChannels {
    pub request_sender: Sender<BrpRequest>,
    pub request_receiver: Receiver<BrpRequest>,
    pub response_sender: Sender<BrpResponse>,
    pub response_receiver: Receiver<BrpResponse>,
}
