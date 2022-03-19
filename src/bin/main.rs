extern crate dotenv;

use dotenv::dotenv;
use serenity::{async_trait, model::prelude::*, prelude::*};

struct Bot;

#[async_trait]
impl EventHandler for Bot {
    async fn message(&self, _ctx: Context, _msg: Message) {
        println!("Message Received!");
    }
}

#[tokio::main]
async fn main() {
    // Import .env vars
    dotenv().ok();

    // Configure the client with your Discord bot token in the environment.
    let token = std::env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let bot = Bot{};

    let mut client = Client::builder(&token).event_handler(bot).await.expect("Err creating client");
    client.start().await.unwrap();
}
