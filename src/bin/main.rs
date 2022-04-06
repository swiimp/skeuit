extern crate dotenv;

use dotenv::dotenv;
use skeuit::{bot::Bot, establish_connection};
use std::{collections::HashMap, env};

#[tokio::main]
async fn main() {
    // Import .env vars
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected DISCORD_TOKEN");
    let os = env::var("DISCORD_OS").expect("Expected DISCORD_OS");
    let intents = env::var("DISCORD_INTENTS")
        .expect("Expected DISCORD_INTENTS")
        .parse::<u64>()
        .expect("Expected DISCORD_INTENTS to be an integer");
    let database_url = env::var("SKEUIT_DATABASE_URL").expect("Expected SKEUIT_DATABASE_URL");
    let mode = env::var("SKEUIT_MODE").expect("Expected SKEUIT_MODE");
    let watched_channels =
        env::var("SKEUIT_WATCHED_CHANNELS").expect("Expected SKEUIT_WATCHED_CHANNELS");

    // Create connection pool
    println!("Establishing database connection...");
    let pool = establish_connection(database_url);
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
