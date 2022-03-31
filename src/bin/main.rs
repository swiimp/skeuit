extern crate dotenv;

use dotenv::dotenv;
use skeuit::{bot::Bot, establish_connection};
use std::{collections::HashMap, env};

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
