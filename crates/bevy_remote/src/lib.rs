use bevy_app::{App, First, MainScheduleOrder, Plugin};
use bevy_ecs::{schedule::ScheduleLabel, system::Resource, world::World};
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

        app.init_schedule(ProcessBrp);

        let mut order = app.world.resource_mut::<MainScheduleOrder>();
        order.insert_after(First, ProcessBrp);

        app.add_systems(ProcessBrp, process_brp_requests);

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

#[derive(Debug, Hash, PartialEq, Eq, Clone, ScheduleLabel)]
pub struct ProcessBrp;

fn process_brp_requests(world: &mut World) {
    let channels = world.resource::<BrpChannels>();

    loop {
        let request = match channels.request_receiver.try_recv() {
            Ok(request) => request,
            Err(err) => match err {
                crossbeam_channel::TryRecvError::Empty => break, // no more requests for now
                crossbeam_channel::TryRecvError::Disconnected => {
                    panic!("BRP request channel disconnected")
                }
            },
        };
        match request.request {
            BrpRequestContent::Ping => {
                channels
                    .response_sender
                    .send(BrpResponse::new(request.id, BrpResponseContent::Ok))
                    .unwrap();
            }
            _ => {
                channels
                    .response_sender
                    .send(BrpResponse::from_error(request.id, BrpError::Unimplemented))
                    .unwrap();
            }
        }
    }
}
