extern crate diesel;
extern crate dotenv;

use std::{
    collections::HashMap,
    env,
    sync::Mutex,
    time::Duration,
};
use async_recursion::async_recursion;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use dotenv::dotenv;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use skeuit::{establish_connection, packet::Packet};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

struct Bot {
    token: String,
    seq_num: Mutex<u64>,
    database: Pool<ConnectionManager<PgConnection>>,
    uri: String,
    heartbeat_int: u64,
    session_id: String,
}

impl Bot {
    pub fn new(
        t: String,
        s: Mutex<u64>,
        pool: Pool<ConnectionManager<PgConnection>>,
        u: String,
    ) -> Bot {
        Bot {
            token: t,
            seq_num: s,
            database: pool,
            uri: u,
            heartbeat_int: 0,
            session_id: "".to_owned(),
        }
    }

    #[async_recursion]
    pub async fn run(&mut self) {
        // Finish connecting
        println!("Starting WS handshake...");
        let addr =
            url::Url::parse(&self.uri).expect("Received bad url");
        let (mut ws_stream, _) = connect_async(&addr).await.expect("Failed to connect");
        let init_msg = ws_stream
            .next()
            .await
            .unwrap()
            .expect("Failed to parse a response");
        // Retrieve and save heartbeat interval
        let resp_packet = Packet::from(init_msg.into_text().unwrap());
        println!("{}", resp_packet.to_string());
        self.extract_and_set_heartbeat(resp_packet);

        let (tx, mut rx) = futures_channel::mpsc::channel::<Packet>(4096);
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let mut interval = tokio::time::interval(Duration::from_millis(self.heartbeat_int));
        let mut has_heartbeat = true;

        loop {
            tokio::select! {
                msg = ws_receiver.next() => {
                    match msg {
                        Some(msg) => {
                            let msg = msg.unwrap();
                            if msg.is_text() || msg.is_binary() {
                                let new_packet = Packet::from(msg.into_text().unwrap());
                                println!("Packet received: {}", new_packet.to_string());
                                self.handle_packet(new_packet).await;
                            } else if msg.is_close() {
                                break;
                            }
                        },
                        None => {},
                    }
                    let x_thread_msg = rx.try_next().unwrap();
                    match x_thread_msg {
                        Some(x_thread_msg) => {
                            // handle cross thread message
                        },
                        None => continue,
                    }
                }
                _ = interval.tick() => {
                    if has_heartbeat {
                        ws_sender.send(Message::Text(self.heartbeat_packet()))
                            .await
                            .expect("Failed to send heartbeat");
                        has_heartbeat = false;
                    } else {
                        break;
                    }
                }
            }
        }
        self.reconnect_or_close(true).await;
    }

    // async helper functions
    async fn get_sequence_number(&self) -> u64 {
        let mut seq_num = self.seq_num.lock().unwrap();
        *seq_num += 1;
        *seq_num
    }

    async fn handle_packet(&self, packet: Packet) {
        println!("Packet received: {}", packet.to_string());
    }

    #[async_recursion]
    async fn reconnect_or_close(&mut self, is_reconnect: bool) {
        if is_reconnect {
            loop {
                // perform reconnect
                break;
            }
            self.run().await;
        }
        // perform close
    }

    // sync helper functions
    fn heartbeat_packet(&self) -> String {
        Packet::new(1, "null".to_owned(), "null".to_owned(), 0).to_string()
    }

    fn extract_and_set_heartbeat(&mut self, init_packet: Packet) {
        let data: Value = serde_json::from_str(&(init_packet.clone()).d).unwrap();
        let heartbeat_interval = serde_json::to_string(&data["heartbeat_interval"])
            .unwrap()
            .parse::<u64>()
            .unwrap();
        println!("heartbeat_interval = {}", heartbeat_interval);
        self.heartbeat_int = heartbeat_interval;
    }
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
        .await
        .expect("Error establishing connection")
        .json::<HashMap<String, String>>()
        .await
        .expect("Failed to parse response");
    println!("URL received, {}", resp["url"]);

    // Instantiate Bot and continue connecting
    let mut bot = Bot::new(token, seq_num, pool, resp["url"].clone() + "?v=9&encoding=json");
    bot.run().await;
}
