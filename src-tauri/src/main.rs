#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod behaviour;

use std::collections::HashSet;
use std::fs;

use serde_json::json;
use tauri::{Manager, State, Window};

use libp2p::{
    core::upgrade,
    floodsub::{Floodsub, FloodsubEvent, Topic},
    futures::StreamExt,
    identity,
    mdns::{Mdns, MdnsEvent},
    mplex,
    noise::{Keypair, NoiseConfig, X25519Spec},
    swarm::{NetworkBehaviourEventProcess, Swarm, SwarmBuilder},
    tcp::TokioTcpConfig,
    NetworkBehaviour, PeerId, Transport,
};
use log::{error, info};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Clone, serde::Serialize)]
struct Payload {
    message: String,
}

const STORAGE_FILE_NAME: &str = "votes.json";

fn get_storage_file_path() -> String {
    format!(
        "{}/{}",
        tauri::api::path::data_dir().unwrap().display(),
        STORAGE_FILE_NAME
    )
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;
type Votes = Vec<Vote>;

static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("votes"));

#[derive(Serialize, Deserialize)]
struct Language {
    name: &'static str,
}

const LANGUAGES: [Language; 21] = [
    Language { name: "Elm" },
    Language { name: "Rust" },
    Language { name: "JavaScript" },
    Language { name: "TypeScript" },
    Language { name: "Elixir" },
    Language { name: "Ruby" },
    Language { name: "OCaml" },
    Language { name: "Python" },
    Language { name: "R" },
    Language { name: "Go" },
    Language { name: "CSharp" },
    Language { name: "Haskell" },
    Language { name: "Clojure" },
    Language { name: "Java" },
    Language { name: "Dart" },
    Language { name: "Julia" },
    Language { name: "Kotlin" },
    Language { name: "Swift" },
    Language { name: "Erlang" },
    Language { name: "Lua" },
    Language { name: "PHP" },
];

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Vote {
    id: usize,
    name: String,
    public: bool,
}

#[derive(Debug, Serialize, Deserialize)]
enum ListMode {
    ALL,
    One(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct ListRequest {
    mode: ListMode,
}

#[derive(Debug, Serialize, Deserialize)]
struct ListResponse {
    mode: ListMode,
    data: Votes,
    receiver: String,
}

enum EventType {
    Response(ListResponse),
}

#[derive(NetworkBehaviour)]
struct VoteBehaviour {
    floodsub: Floodsub,
    mdns: Mdns,
    #[behaviour(ignore)]
    response_sender: mpsc::UnboundedSender<ListResponse>,
}

struct SenderState {
    sender: mpsc::UnboundedSender<ListResponse>,
}

impl NetworkBehaviourEventProcess<FloodsubEvent> for VoteBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(msg) => {
                if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                    if resp.receiver == PEER_ID.to_string() {
                        info!("Response from {}:", msg.source);
                        resp.data.iter().for_each(|r| info!("{:?}", r));
                    }
                } else if let Ok(req) = serde_json::from_slice::<ListRequest>(&msg.data) {
                    match req.mode {
                        ListMode::ALL => {
                            info!("Received ALL req: {:?} from {:?}", req, msg.source);
                            respond_with_public_votes(
                                self.response_sender.clone(),
                                msg.source.to_string(),
                            );
                        }
                        ListMode::One(ref peer_id) => {
                            if peer_id == &PEER_ID.to_string() {
                                info!("Received req: {:?} from {:?}", req, msg.source);
                                respond_with_public_votes(
                                    self.response_sender.clone(),
                                    msg.source.to_string(),
                                );
                            }
                        }
                    }
                }
            }
            _ => (),
        }
    }
}

fn respond_with_public_votes(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match read_local_votes() {
            Ok(votes) => {
                let resp = ListResponse {
                    mode: ListMode::ALL,
                    receiver,
                    data: votes.into_iter().filter(|r| r.public).collect(),
                };
                if let Err(e) = sender.send(resp) {
                    error!("error sending response via channel, {}", e);
                }
            }
            Err(e) => error!("error fetching local votes to answer ALL request, {}", e),
        }
    });
}

impl NetworkBehaviourEventProcess<MdnsEvent> for VoteBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered_list) => {
                for (peer, _addr) in discovered_list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) => {
                for (peer, _addr) in expired_list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}

fn add_vote(name: &str) -> Result<Vote> {
    let mut local_votes = read_local_votes()?;
    info!("{:?}", local_votes);
    let new_id = match local_votes.iter().max_by_key(|r| r.id) {
        Some(v) => v.id + 1,
        None => 0,
    };
    let vote = Vote {
        id: new_id,
        name: name.to_owned(),
        public: false,
    };
    local_votes.push(vote.clone());
    write_local_votes(&local_votes)?;

    info!("Added vote:");
    info!("Name: {}", name);

    Ok(vote)
}

async fn publish_vote(id: usize) -> Result<()> {
    let mut local_votes = read_local_votes()?;
    local_votes
        .iter_mut()
        .filter(|r| r.id == id)
        .for_each(|r| r.public = true);
    write_local_votes(&local_votes)?;
    Ok(())
}

fn read_local_votes() -> Result<Votes> {
    match fs::read(get_storage_file_path()) {
        Ok(votes) => Ok(serde_json::from_slice(&votes)?),
        Err(_e) => Ok(vec![]),
    }
}

fn write_local_votes(votes: &Votes) -> Result<()> {
    let json = serde_json::to_string(&votes)?;

    fs::write(get_storage_file_path(), &json)?;
    Ok(())
}

async fn handle_list_peers(swarm: &mut Swarm<VoteBehaviour>) {
    info!("Discovered Peers:");
    let nodes = swarm.behaviour().mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| info!("{}", p));
}

async fn handle_list_recipes(cmd: &str, swarm: &mut Swarm<VoteBehaviour>) {
    let rest = cmd.strip_prefix("ls r ");
    match rest {
        Some("all") => {
            let req = ListRequest {
                mode: ListMode::ALL,
            };
            let json = serde_json::to_string(&req).expect("cannot jsonify request");
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json.as_bytes());
        }
        Some(recipes_peer_id) => {
            let req = ListRequest {
                mode: ListMode::One(recipes_peer_id.to_owned()),
            };
            let json = serde_json::to_string(&req).expect("cannot jsonify request");
            swarm
                .behaviour_mut()
                .floodsub
                .publish(TOPIC.clone(), json.as_bytes());
        }
        None => {
            match read_local_votes() {
                Ok(v) => {
                    info!("Local Recipes ({})", v.len());
                    v.iter().for_each(|r| info!("{:?}", r));
                }
                Err(e) => error!("error fetching local votes: {}", e),
            };
        }
    };
}

#[tauri::command]
fn on_publish_vote(name: String, window: Window, state: State<SenderState>) -> tauri::Result<()> {
    add_vote(name.as_str()).expect("Could not write");

    let cloned_state = state.sender.clone();

    tauri::async_runtime::spawn(async move {
        respond_with_public_votes(cloned_state, String::from("any"));
    });

    window
        .emit(
            "get_votes",
            json!({
                "votes": read_local_votes().unwrap(),
            }),
        )
        .expect("failed to emit get_votes event");

    Ok(())
}

async fn initialize(window: &Window) {
    info!("Peer Id: {}", PEER_ID.clone());
    let (response_sender, mut response_rcv) = mpsc::unbounded_channel();

    window.manage(SenderState {
        sender: response_sender,
    });

    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(&KEYS)
        .expect("can't create auth keys");

    let transp = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated()) // XX Handshake pattern, IX exists as well and IK - only XX currently provides interop with other libp2p impls
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    let mut behaviour = behaviour::Behaviour::new(PEER_ID.clone()).await;

    behaviour.floodsub.subscribe(TOPIC.clone());

    let mut swarm = SwarmBuilder::new(transp, behaviour, PEER_ID.clone())
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("can't get a local socket"),
    )
    .expect("swarm can't be started");

    loop {
        let evt = {
            tokio::select! {
                response = response_rcv.recv() => Some(EventType::Response(response.expect("response doesn't exist"))),
                event = swarm.select_next_some() => {
                    info!("Unhandled Swarm Event: {:?}", event);
                    None
                },
            }
        };

        if let Some(event) = evt {
            match event {
                EventType::Response(resp) => {
                    let json = serde_json::to_string(&resp).expect("cannot jsonify response");
                    println!("Received data {:?}", json);
                    window.emit("new", &json).unwrap();
                }
            }
        }
    }
}

fn main() {
    pretty_env_logger::init();

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.get_window("main").unwrap().open_devtools();

            let window = app.get_window("main").unwrap();

            let wintwo = app.get_window("main").unwrap();

            app.get_window("main").unwrap().listen("ping", move |_| {
                wintwo
                    .emit(
                        "get_languages",
                        json!({
                            "languages": LANGUAGES,
                        }),
                    )
                    .expect("failed to emit get_votes event");
                wintwo
                    .emit(
                        "get_votes",
                        json!({
                            "votes": read_local_votes().unwrap(),
                        }),
                    )
                    .expect("failed to emit get_votes event");
            });

            tauri::async_runtime::spawn(async move {
                initialize(&window).await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![on_publish_vote])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
