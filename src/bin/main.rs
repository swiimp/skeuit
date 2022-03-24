extern crate diesel;
extern crate dotenv;

use std::{
    collections::HashMap,
    env,
    // io::Error as IoError,
    // net::SocketAddr,
    sync::Mutex,
};
// use diesel::prelude::*;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, /* PooledConnection */};
use dotenv::dotenv;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, WebSocketStream};

use skeuit::establish_connection;
// use skeuit::models::*;

struct Bot {
    token: String,
    seq_num: Mutex<u64>,
    heartbeat_int: u64,
    session_id: String,
    database: Pool<ConnectionManager<PgConnection>>,
    stream: WebSocketStream<TcpStream>,
}

struct Packet {
    op: u64,
    d:  String,
    t:  String,
    s:  u64,
}

impl Bot {
    // pub fn new() -> Bot {
    //     Bot{}
    // }

    async fn get_sequence_number(&self) -> u64 {
        let mut seq_num = self.seq_num.lock().unwrap();
        *seq_num += 1;
        *seq_num
    }
}

impl Packet {
    pub fn new(op_code: u64, data: String, transaction_type: String, seq_num: u64) -> Packet {
        Packet{ op: op_code, d: data, t: transaction_type, s: seq_num}
    }

    pub fn to_string(&self) -> String {
        let mut result: String;
        if self.op != 0 {
            result = format!("\"op\":{},\"d\":{},\"t\":null,\"s\":null", self.op, self.d);
        }
        result = format!("\"op\":{},\"d\":{},\"t\":{},\"s\":{}", self.op, self.d, self.t, self.s);
        "{".to_string() + &result + "}"
    }
}

#[tokio::main]
async fn main() {
    // Create initial sequence number
    let seq_num = Mutex::new(1u64);

    // Import .env vars
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected a token");

    // Create connection pool
    println!("Establishing database connection...");
    let pool = establish_connection();
    println!("Database connection successfully established");

    // Get WS address from Discord API
    println!("Establishing connection to Discord...");
    let resp = reqwest::get("https://discord.com/api/v9/gateway")
        .await.unwrap()
        .json::<HashMap<String, String>>()
        .await.unwrap();
    println!("URI received, {}", resp["url"]);

    println!("Starting WS handshake...");
    let addr = url::Url::parse(&(resp["url"].clone() + "?v=9&encoding=json")).unwrap();
    let (mut ws_stream, _) = connect_async(&addr).await.expect("Failed to connect");
    let init_request = Packet::new(1, "null".to_string(), "null".to_string(), 0);

    ws_stream.send(Message::text(init_request.to_string())).await.expect("Failed to initiate handshake");

    while let Some(msg) = ws_stream.next().await {
        let msg = msg.unwrap();
        println!("{:?}", msg);
    }
}
