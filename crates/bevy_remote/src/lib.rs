use bevy_app::{App, First, MainScheduleOrder, Plugin};
use bevy_ecs::{schedule::ScheduleLabel, system::Resource, world::World};
use bevy_log::debug;
use brp::*;
use crossbeam_channel::{Receiver, Sender};

pub mod brp;

#[cfg(feature = "http")]
pub mod http;

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(ProcessBrp);

        let mut order = app.world.resource_mut::<MainScheduleOrder>();
        order.insert_after(First, ProcessBrp);

        app.add_systems(ProcessBrp, process_brp_requests);

        app.insert_resource(BrpSessions::default());

        #[cfg(feature = "http")]
        app.add_plugins(http::HttpRemotePlugin);
    }
}

#[derive(Resource, Default)]
pub struct BrpSessions(Vec<BrpSession>);

#[derive(Debug, Clone)]
pub struct BrpSession {
    pub label: String,
    pub request_sender: Sender<BrpRequest>,
    pub request_receiver: Receiver<BrpRequest>,
    pub response_sender: Sender<BrpResponse>,
    pub response_receiver: Receiver<BrpResponse>,
}

impl BrpSessions {
    pub fn open(&mut self, label: impl Into<String>) -> BrpSession {
        let (request_sender, request_receiver) = crossbeam_channel::unbounded();
        let (response_sender, response_receiver) = crossbeam_channel::unbounded();

        let session = BrpSession {
            label: label.into(),
            request_sender,
            request_receiver,
            response_sender,
            response_receiver,
        };

        for existing_session in self.0.iter() {
            assert_ne!(existing_session.label, session.label);
        }

        self.0.push(session.clone());

        session
    }

    pub fn close(&mut self, label: &str) {
        let index = self
            .0
            .iter()
            .position(|session| session.label == label)
            .unwrap();

        self.0.remove(index);
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, ScheduleLabel)]
pub struct ProcessBrp;

fn process_brp_requests(world: &mut World) {
    let sessions = world.resource::<BrpSessions>();

    for session in sessions.0.iter() {
        loop {
            let request = match session.request_receiver.try_recv() {
                Ok(request) => request,
                Err(err) => match err {
                    crossbeam_channel::TryRecvError::Empty => break, // no more requests for now
                    crossbeam_channel::TryRecvError::Disconnected => {
                        panic!("BRP request channel disconnected")
                    }
                },
            };

            debug!("Received {:?} from session {:?}", request, session.label);

            match request.request {
                BrpRequestContent::Ping => {
                    session
                        .response_sender
                        .send(BrpResponse::new(request.id, BrpResponseContent::Ok))
                        .unwrap();
                }
                _ => {
                    session
                        .response_sender
                        .send(BrpResponse::from_error(request.id, BrpError::Unimplemented))
                        .unwrap();
                }
            }
        }
    }
}
