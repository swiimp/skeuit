extern crate diesel;
extern crate dotenv;

use std::{
    collections::HashMap,
    env,
    // io::Error as IoError,
    net::SocketAddr,
    sync::Mutex,
};
// use diesel::prelude::*;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use dotenv::dotenv;
use futures_util::{future, pin_mut, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use skeuit::establish_connection;
// use skeuit::models::*;

struct Bot {
    token: String,
    seq_num: Mutex<u64>,
    heartbeat_int: u64,
    session_id: String,
    database: Pool<ConnectionManager<PgConnection>>,
}

struct Packet {
    op: u64,
    d:  String,
    s:  u64,
    t:  String,
}

impl Bot {
    pub async fn get_sequence_number(&self) -> u64 {
        let mut seq_num = self.seq_num.lock().unwrap();
        *seq_num += 1;
        *seq_num
    }
}

impl Packet {
    pub fn new(op_code: u64, data: String, seq_num: u64, type: String) -> Packet {
        Packet{ op: op_code, d: data, s: seq_num, t: type }
    }

    pub fn to_string(&self) -> String {
        if self.op != 0 {
            format!("{\"op\":{},\"d\":{},\"t\":null,\"s\":null}", self.op, self.print_d())
        }
        format!("{\"op\":{},\"d\":{},\"t\":{},\"s\":{}}", self.op, self.print_d(), self.t, self.s)
    }

    fn print_d(&self) -> String {
        if self.d == '' {
            "null"
        }
        self.d
    }
}

async fn read_stdin(tx: futures_channel::mpsc::UnboundedSender<Message>) {
    let mut stdin = tokio::io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf).await {
            Err(_) | Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);
        tx.unbounded_send(Message::binary(buf)).unwrap();
    }
}

#[tokio::main]
async fn main() {
    // use skeuit::schema::messages::dsl::*;

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
    let (stdin_tx, stdin_rx) = futures_channel::mpsc::unbounded();
    tokio::spawn(read_stdin(stdin_tx));
    let (ws_stream, resp) = connect_async(addr).await.expect("Failed to connect");
    println!("{:?}", resp);

    let (write, read) = ws_stream.split();

    let stdin_to_ws = stdin_rx.map(Ok).forward(write);
    let ws_to_stdout = {
        read.for_each(|message| async {
            let data = message.unwrap().into_data();
            tokio::io::stdout().write_all(&data).await.unwrap();
        })
    };

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;
    println!("WebSocket handshake has been successfully completed");
}
