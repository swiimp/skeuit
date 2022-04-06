use crate::packet::Packet;
use async_recursion::async_recursion;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Copy, Clone)]
enum Flag {
    Heartbeat,
    Reconnect,
}

pub struct Bot {
    token: String,
    os: String,
    intents: u64,
    seq_num: u64,
    database: Pool<ConnectionManager<PgConnection>>,
    uri: String,
    mode: String,
    watched_channels: Vec<u64>,
    jobs: Vec<Packet>,
    heartbeat_int: u64,
    session_id: String,
    flags: u8,
}

impl Bot {
    pub fn new(
        t: String,
        o: String,
        i: u64,
        m: String,
        w_c: String,
        pool: Pool<ConnectionManager<PgConnection>>,
        u: String,
    ) -> Bot {
        Bot {
            token: t,
            os: o,
            intents: i,
            seq_num: 0,
            database: pool,
            uri: u,
            mode: m,
            watched_channels: w_c
                .split(",")
                .map(|str| str.parse::<u64>().unwrap())
                .collect(),
            jobs: vec![],
            heartbeat_int: 0,
            session_id: "".to_owned(),
            flags: 0,
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
            }
        };
        self.extract_and_set_heartbeat(init_packet);
        self.set_flag(Flag::Heartbeat, true);

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
            let id_packet = match Packet::from(id_msg.into_text().unwrap()) {
                Ok(packet) => packet,
                Err(error) => {
                    panic!("Problem reading packet data: {:?}", error)
                }
            };
            self.extract_and_set_session_id_and_seq_num(id_packet);
        } else {
            println!("session_id found. Resuming session...");
            // Resume old session
            ws_stream
                .send(Message::Text(self.resume_packet()))
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
                                continue;
                            }
                        };
                        println!("REPLAY Packet received: {}", packet.to_string());
                        if packet.op == 0 {
                            self.queue_job(packet);
                        }
                    }
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
                                        self.set_seq_num(packet.s);
                                        self.queue_job(packet);
                                    },
                                    1 => {
                                        self.set_flag(Flag::Heartbeat, true);
                                    },
                                    7 => {
                                        self.set_flag(Flag::Reconnect, true);
                                        break;
                                    },
                                    9 => {
                                        self.session_id = String::from("");
                                        self.set_flag(Flag::Reconnect, true);
                                        break;
                                    },
                                    11 => {
                                        self.set_flag(Flag::Heartbeat, true);
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
                    if self.check_flag(Flag::Heartbeat) {
                        println!("HEARTBEAT SUCCESSFUL.");
                        ws_sender.send(Message::Text(self.heartbeat_packet()))
                            .await
                            .expect("Failed to send heartbeat");
                        self.set_flag(Flag::Heartbeat, false);
                    } else {
                        println!("HEARTBEAT FAILED. ZOMBIFIED CONNECTION.");
                        ws_sender.close()
                            .await
                            .expect("Failed to close connection");
                        self.set_flag(Flag::Reconnect, true);
                        break;
                    }
                }
            }
            println!("Checking job queue...");
            // TODO: Batch jobs and send to another thread
            loop {
                match self.retrieve_job() {
                    Some(packet) => {
                        self.handle_packet(packet);
                    }
                    None => break,
                }
            }
        }
        if self.check_flag(Flag::Reconnect) {
            self.set_flag(Flag::Reconnect, false);
            self.run().await;
        }
    }

    fn handle_packet(&self, packet: Packet) {
        println!("handle_packet :: {}", packet.to_string());
    }

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

    fn resume_packet(&self) -> String {
        let data = json!({
            "token": self.token,
            "session_id": self.session_id,
            "seq": self.seq_num
        });
        Packet::new(6, data.to_string(), "null".to_owned(), 0).to_string()
    }

    fn extract_and_set_heartbeat(&mut self, init_packet: Packet) {
        let data: Value = serde_json::from_str(&(init_packet.clone()).d).unwrap();
        self.heartbeat_int = serde_json::to_string(&data["heartbeat_interval"])
            .unwrap()
            .parse::<u64>()
            .unwrap();
    }

    fn extract_and_set_session_id_and_seq_num(&mut self, id_packet: Packet) {
        let data: Value = serde_json::from_str(&(id_packet.to_string())).unwrap();
        self.session_id = serde_json::to_string(&data["d"]["session_id"]).unwrap();
        self.set_seq_num(
            serde_json::to_string(&data["s"])
                .unwrap()
                .parse::<u64>()
                .unwrap(),
        );
    }

    fn queue_job(&mut self, packet: Packet) {
        self.jobs.push(packet);
    }

    fn retrieve_job(&mut self) -> Option<Packet> {
        if self.jobs.len() > 0 {
            return Some(self.jobs.remove(0));
        }
        None
    }

    fn set_seq_num(&mut self, s_num: u64) {
        if s_num > self.seq_num {
            self.seq_num = s_num;
        }
    }

    fn check_flag(&self, flag: Flag) -> bool {
        let mask = self.get_mask(flag);
        self.flags & mask > 0
    }

    fn set_flag(&mut self, flag: Flag, value: bool) {
        let mask = self.get_mask(flag);
        if self.check_flag(flag) != value {
            self.flags = self.flags ^ mask;
        }
    }

    fn get_mask(&self, key: Flag) -> u8 {
        match key {
            Flag::Heartbeat => 1,
            Flag::Reconnect => 2,
        }
    }
}
