extern crate diesel;
extern crate dotenv;

use std::{
    collections::HashMap,
    env,
    // io::Error as IoError,
    // net::SocketAddr,
    sync::Mutex,
    time::Duration,
};
// use diesel::prelude::*;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, /* PooledConnection */};
use dotenv::dotenv;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream};
use skeuit::{establish_connection, packet::Packet};
// use skeuit::models::*;

struct Bot {
    token: String,
    seq_num: Mutex<u64>,
    heartbeat_int: u64,
    session_id: String,
    database: Pool<ConnectionManager<PgConnection>>,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    connected: bool,
}

impl Bot {
    pub fn new(t: String, s: Mutex<u64>, heartbeat: u64,
    pool: Pool<ConnectionManager<PgConnection>>,
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Bot {
        Bot{
            token: t,
            seq_num: s,
            heartbeat_int: heartbeat,
            session_id: "".to_owned(),
            database: pool,
            ws: ws,
            connected: false,
        }
    }

    pub async fn run(&self) {
        let (tx, mut rx) = futures_channel::mpsc::channel::<Packet>(4096);
        let (mut ws_sender, mut ws_receiver) = self.ws.split();
        let mut interval = tokio::time::interval(Duration::from_millis(self.heartbeat_int));
        let mut has_heartbeat = true;

        loop {
            tokio::select! {
                msg = ws_receiver.next() => {
                    match msg {
                        Some(msg) => {
                            let msg = msg.unwrap();
                            if msg.is_text() || msg.is_binary() {
                                self.handle_packet(Packet::from(msg.into_text().unwrap()));
                            } else if msg.is_close() {
                                break;
                            }
                        },
                        None => {
                            let x_thread_msg = rx.try_next().unwrap();
                            match x_thread_msg {
                                Some(x_thread_msg) => {
                                    // handle cross thread message
                                },
                                None => continue,
                            }
                        },
                    }
                }
                _ = interval.tick() => {
                    if has_heartbeat {
                        ws_sender.send(Message::Text(self.heartbeat()))
                            .await
                            .expect("Failed to send heartbeat");
                        has_heartbeat = false;
                    } else {
                        break;
                    }
                }
            }
        }
        self.reconnect_or_close(true);
    }

    async fn get_sequence_number(&self) -> u64 {
        let mut seq_num = self.seq_num.lock().unwrap();
        *seq_num += 1;
        *seq_num
    }

    /*
    some amount of handlers for tasks
    */
    async fn handle_packet(&self, packet: Packet) {
        println!("Packet received: {}", packet.to_string());
    }

    fn heartbeat(&self) -> String {
        Packet::new(1, "null".to_owned(), "null".to_owned(), 0).to_string()
    }

    async fn reconnect_or_close(&self, is_reconnect: bool) {
        if is_reconnect {
            loop {
                // perform reconnect
                break;
            }
            self.run();
        }
        // perform close
    }
}

fn extract_heartbeat(init_packet: Packet) -> u64 {
    let data: Value = serde_json::from_str(&(init_packet.clone()).d).unwrap();
    serde_json::to_string(&data["heartbeat_interval"]).unwrap().parse::<u64>().unwrap()
}

#[tokio::main]
async fn main() {
    // Import .env vars
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected a token");

    // Create initial sequence number
    let seq_num = Mutex::new(1u64);

    // Create connection pool
    println!("Establishing database connection...");
    let pool = establish_connection();
    println!("Database connection successfully established");

    // Get WS address from Discord API
    println!("Establishing connection to Discord...");
    let resp = reqwest::get("https://discord.com/api/v9/gateway")
        .await.expect("Error establishing connection")
        .json::<HashMap<String, String>>()
        .await.expect("Failed to parse response");
    println!("URI received, {}", resp["url"]);

    println!("Starting WS handshake...");
    let addr = url::Url::parse(&(resp["url"].clone() + "?v=9&encoding=json")).expect("Received bad url");
    let (mut ws_stream, _) = connect_async(&addr).await.expect("Failed to connect");
    let init_msg = ws_stream.next().await.unwrap().expect("Failed to parse a response");
    let resp_packet = Packet::from(init_msg.into_text().unwrap());
    println!("{}", resp_packet.to_string());
    let heartbeat_interval = extract_heartbeat(resp_packet);
    println!("heartbeat_interval = {}", heartbeat_interval);

    let bot = Bot::new(token, seq_num, heartbeat_interval, pool, ws_stream);
    // bot.run();
}
