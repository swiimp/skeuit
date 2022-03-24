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
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream};

use skeuit::establish_connection;
// use skeuit::models::*;

struct Bot {
    token: String,
    seq_num: Mutex<u64>,
    heartbeat_int: u64,
    session_id: String,
    database: Pool<ConnectionManager<PgConnection>>,
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

struct Packet {
    op: u64,
    d:  String,
    t:  String,
    s:  u64,
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
            stream: ws,
        }
    }

    pub fn run(&self) {

    }

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

    pub fn from(packet_data: String) -> Packet {
        let json: Value = serde_json::from_str(&packet_data).unwrap();
        let mut seq_num: u64 = 0;
        if serde_json::to_string(&json["s"]).unwrap() != "null" {
            seq_num = serde_json::to_string(&json["s"]).unwrap().parse::<u64>().unwrap();
        }
        Packet{
            op: serde_json::to_string(&json["op"]).unwrap().parse::<u64>().unwrap(),
            d: serde_json::to_string(&json["d"]).unwrap(),
            t: serde_json::to_string(&json["t"]).unwrap(),
            s: seq_num,
        }
    }

    pub fn to_string(&self) -> String {
        let result: String;
        if self.op != 0 {
            result = format!("\"op\":{},\"d\":{},\"t\":null,\"s\":null", self.op, self.d);
        } else {
            result = format!("\"op\":{},\"d\":{},\"t\":{},\"s\":{}", self.op, self.d, self.t, self.s);
        }
        "{".to_owned() + &result + "}"
    }
}

fn extract_heartbeat(init_packet: Packet) -> u64 {

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
    let init_request = Packet::new(1, "null".to_owned(), "null".to_owned(), 0);
    let init_msg = ws_stream.next().await.expect("Failed to parse a response");
    let resp_packet = Packet::from(init_msg.into_text().unwrap());
    let heartbeat_interval = extract_heartbeat(resp_packet);

    let bot = Bot::new(token, seq_num, heartbeat_interval, pool, ws_stream);
    bot.run();
}
