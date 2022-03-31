extern crate diesel;
extern crate dotenv;

use async_recursion::async_recursion;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use dotenv::dotenv;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use skeuit::{establish_connection, packet::Packet};
use std::{collections::HashMap, env, sync::Mutex, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

enum Flag {
    Heartbeat,
}

struct Bot {
    token: String,
    os: String,
    intents: u64,
    seq_num: Mutex<u64>,
    database: Pool<ConnectionManager<PgConnection>>,
    uri: String,
    heartbeat_int: u64,
    session_id: String,
    flags: Mutex<u8>,
}

impl Bot {
    pub fn new(
        t: String,
        o: String,
        i: u64,
        pool: Pool<ConnectionManager<PgConnection>>,
        u: String,
    ) -> Bot {
        Bot {
            token: t,
            os: o,
            intents: i,
            seq_num: Mutex::new(0u64),
            database: pool,
            uri: u,
            heartbeat_int: 0,
            session_id: "".to_owned(),
            flags: Mutex::new(0u8),
        }
    }

    #[async_recursion]
    pub async fn run(&mut self) {
        // Finish connecting
        println!("Starting WS handshake...");
        let addr = url::Url::parse(&self.uri).expect("Received bad url");
        let (mut ws_stream, _) = connect_async(&addr).await.expect("Failed to connect");
        if self.heartbeat_int == 0 {
            let init_msg = ws_stream
                .next()
                .await
                .unwrap()
                .expect("Failed to parse an initial response");
            // Retrieve and save heartbeat interval
            let init_packet = Packet::from(init_msg.into_text().unwrap());
            self.extract_and_set_heartbeat(init_packet);
            self.set_flag(Flag::Heartbeat, true).await;
        }
        if self.session_id == "" {
            // Identify
            ws_stream
                .send(Message::Text(self.id_packet()))
                .await
                .expect("Failed to send 'Identify' packet");
            // Receive session_id
            let id_msg = ws_stream
                .next()
                .await
                .unwrap()
                .expect("Failed to parse an ID response");
            let id_packet = Packet::from(id_msg.into_text().unwrap());
            self.extract_and_set_session_id_and_seq_num(id_packet).await;
        }
        println!("Handshake complete!");

        let (tx, mut rx) = futures_channel::mpsc::channel::<Packet>(4096);
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let mut interval = tokio::time::interval(Duration::from_millis(self.heartbeat_int));

        loop {
            tokio::select! {
                msg = ws_receiver.next() => {
                    match msg {
                        Some(msg) => {
                            let msg = msg.unwrap();
                            if msg.is_text() || msg.is_binary() {
                                let packet = Packet::from(msg.into_text().unwrap());
                                println!("Packet received: {}", packet.to_string());
                                match packet.op {
                                    11 => {
                                        self.set_flag(Flag::Heartbeat, true).await;
                                    },
                                    _ => {},
                                }
                            } else if msg.is_close() {
                                println!("CONNECTION CLOSED.");
                                break;
                            }
                        },
                        None => {},
                    }
                    // let x_thread_msg = rx.try_next().unwrap();
                    // match x_thread_msg {
                    //     Some(x_thread_msg) => {
                    //         // handle cross thread message
                    //     },
                    //     None => continue,
                    // }
                }
                _ = interval.tick() => {
                    if self.check_flag(Flag::Heartbeat).await > 0 {
                        println!("HEARTBEAT SUCCESSFUL.");
                        ws_sender.send(Message::Text(self.heartbeat_packet()))
                            .await
                            .expect("Failed to send heartbeat");
                        self.set_flag(Flag::Heartbeat, false).await;
                    } else {
                        println!("HEARTBEAT FAILED. ZOMBIFIED CONNECTION.");
                        break;
                    }
                }
            }
        }
        self.reconnect_or_close(true).await;
    }

    // async helper functions
    async fn set_seq_num(&self, s_num: u64) {
        let mut seq_num = self.seq_num.lock().unwrap();
        *seq_num = s_num;
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

    async fn check_flag(&self, flag: Flag) -> u8 {
        let flags = self.flags.lock().unwrap();
        let mask = self.get_mask(flag);
        *flags & mask
    }

    async fn set_flag(&mut self, flag: Flag, value: bool) {
        let mut flags = self.flags.lock().unwrap();
        let mask = self.get_mask(flag);
        let is_set = *flags & mask;
        if !((is_set > 0) == value) {
            *flags = *flags ^ mask;
        }
    }

    async fn extract_and_set_session_id_and_seq_num(&mut self, id_packet: Packet) {
        let data: Value = serde_json::from_str(&(id_packet.to_string())).unwrap();
        self.session_id = serde_json::to_string(&data["d"]["session_id"]).unwrap();
        self.set_seq_num(
            serde_json::to_string(&data["s"])
                .unwrap()
                .parse::<u64>()
                .unwrap(),
        )
        .await;
    }

    // sync helper functions
    fn heartbeat_packet(&self) -> String {
        Packet::new(1, "null".to_owned(), "null".to_owned(), 0).to_string()
    }

    fn id_packet(&self) -> String {
        let data = json!({
            "token": self.token,
            "properties": {
                "$os": self.os,
                "$browser": "skeuit",
                "$device": "skeuit"
            },
            "intents": self.intents
        });
        Packet::new(2, data.to_string(), "null".to_owned(), 0).to_string()
    }

    fn extract_and_set_heartbeat(&mut self, init_packet: Packet) {
        let data: Value = serde_json::from_str(&(init_packet.clone()).d).unwrap();
        self.heartbeat_int = serde_json::to_string(&data["heartbeat_interval"])
            .unwrap()
            .parse::<u64>()
            .unwrap();
    }

    fn get_mask(&self, key: Flag) -> u8 {
        match key {
            Flag::Heartbeat => 1,
        }
    }
}

#[tokio::main]
async fn main() {
    // Import .env vars
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected a token");
    let os = env::var("DISCORD_OS").expect("Expected an os");
    let intents = env::var("DISCORD_INTENTS")
        .unwrap()
        .parse::<u64>()
        .expect("Expected intents integer");

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
    let mut bot = Bot::new(
        token,
        os,
        intents,
        pool,
        format!("{}?v=9&encoding=json", resp["url"]),
    );
    bot.run().await;
}
