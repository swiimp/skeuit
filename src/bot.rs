use async_recursion::async_recursion;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use crate::packet::Packet;
use std::{sync::Mutex, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

enum Flag {
    Heartbeat,
    Reconnect,
}

pub struct Bot {
    token: String,
    os: String,
    intents: u64,
    seq_num: Mutex<u64>,
    database: Pool<ConnectionManager<PgConnection>>,
    uri: String,
    jobs: Mutex<Vec<Packet>>,
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
            seq_num: Mutex::new(0),
            database: pool,
            uri: u,
            jobs: Mutex::new(vec![]),
            heartbeat_int: 0,
            session_id: "".to_owned(),
            flags: Mutex::new(0),
        }
    }

    #[async_recursion]
    pub async fn run(&mut self) {
        // Finish connecting
        println!("Starting WS handshake...");
        let addr = url::Url::parse(&self.uri).expect("Received bad url");
        let (mut ws_stream, _) = connect_async(&addr).await.expect("Failed to connect");
        let init_msg = ws_stream
            .next()
            .await
            .unwrap()
            .expect("Failed to parse an initial response");
        // Retrieve and save heartbeat interval
        let init_packet = match Packet::from(init_msg.into_text().unwrap()) {
            Ok(packet) => packet,
            Err(error) => {
                panic!("Problem reading packet data: {:?}", error)
            },
        };
        self.extract_and_set_heartbeat(init_packet);
        self.set_flag(Flag::Heartbeat, true).await;

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
            let id_packet = Packet::from(id_msg.into_text().unwrap()).unwrap();
            self.extract_and_set_session_id_and_seq_num(id_packet).await;
        } else {
            println!("session_id found. Resuming session...");
            // Resume old session
            ws_stream
                .send(Message::Text(self.resume_packet().await))
                .await
                .expect("Failed to resume session");
            // Send replay to queue
            println!("Replaying missed events...");
            loop {
                match ws_stream.next().await {
                    Some(msg) => {
                        let packet = match Packet::from(msg.unwrap().into_text().unwrap()) {
                            Ok(packet) => packet,
                            Err(error) => {
                                println!("Problem reading packet data: {:?}", error);
                                // TODO: Find a permanent solution
                                Packet::from(self.heartbeat_packet()).unwrap()
                            },
                        };
                        println!("REPLAY Packet received: {}", packet.to_string());
                        if packet.op == 0 {
                            self.queue_job(packet).await;
                        }
                    },
                    None => break,
                }
            }
        }
        println!("Handshake complete!");

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let mut interval = tokio::time::interval(Duration::from_millis(self.heartbeat_int));

        loop {
            tokio::select! {
                msg = ws_receiver.next() => {
                    match msg {
                        Some(msg) => {
                            let msg = msg.unwrap();
                            if msg.is_text() || msg.is_binary() {
                                let packet = match Packet::from(msg.into_text().unwrap()) {
                                    Ok(packet) => packet,
                                    Err(error) => {
                                        println!("Problem reading packet data: {:?}", error);
                                        continue;
                                    },
                                };
                                println!("Packet received: {}", packet.to_string());
                                match packet.op {
                                    0 => {
                                        self.set_seq_num(packet.s).await;
                                        self.queue_job(packet).await;
                                    },
                                    1 => {
                                        self.set_flag(Flag::Heartbeat, true).await;
                                    },
                                    7 => {
                                        self.set_flag(Flag::Reconnect, true).await;
                                        break;
                                    },
                                    9 => {
                                        self.session_id = String::from("");
                                        self.set_flag(Flag::Reconnect, true).await;
                                        break;
                                    },
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
                        ws_sender.close()
                            .await
                            .expect("Failed to close connection");
                        self.set_flag(Flag::Reconnect, true).await;
                        break;
                    }
                }
            }
            println!("Checking job queue...");
            loop {
                match self.retrieve_job().await {
                    Some(packet) => {
                        println!("{}", packet.to_string());
                    },
                    None => break,
                }
            }
        }
        if self.check_flag(Flag::Reconnect).await > 0 {
            self.set_flag(Flag::Reconnect, false).await;
            self.run().await;
        }
    }

    // async helper functions
    async fn set_seq_num(&self, s_num: u64) {
        let mut seq_num = self.seq_num.lock().unwrap();
        if s_num > *seq_num {
            *seq_num = s_num;
        }
    }

    async fn check_flag(&self, flag: Flag) -> u8 {
        let flags = self.flags.lock().unwrap();
        let mask = self.get_mask(flag);
        *flags & mask
    }

    async fn set_flag(&self, flag: Flag, value: bool) {
        let mut flags = self.flags.lock().unwrap();
        let mask = self.get_mask(flag);
        let is_set = *flags & mask;
        if !((is_set > 0) == value) {
            *flags = *flags ^ mask;
        }
    }

    async fn queue_job(&self, packet: Packet) {
        let mut jobs = self.jobs.lock().unwrap();
        jobs.push(packet);
    }

    async fn retrieve_job(&self) -> Option<Packet> {
        let mut jobs = self.jobs.lock().unwrap();
        if jobs.len() > 0 {
            return Some(jobs.remove(0))
        }
        None
    }

    async fn resume_packet(&self) -> String {
        let seq_num = self.seq_num.lock().unwrap();
        let data = json!({
            "token": self.token,
            "session_id": self.session_id,
            "seq": *seq_num
        });
        Packet::new(6, data.to_string(), "null".to_owned(), 0).to_string()
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
            Flag::Reconnect => 2,
        }
    }
}
