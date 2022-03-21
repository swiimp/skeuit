extern crate diesel;
extern crate dotenv;

use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use dotenv::dotenv;
use skeuit::*;
use skeuit::models::*;

struct Bot{
    database: Pool<ConnectionManager<PgConnection>>,
}
#[tokio::main]
async fn main() {
    use skeuit::schema::messages::dsl::*;

    // Import .env vars
    dotenv().ok();

    // Configure the client with your Discord bot token in the environment.
    // let token = std::env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    println!("Establishing database connection...");

    // Create connection pool
    let connection = establish_connection();

    let results = messages.limit(5)
        .load::<Message>(&connection)
        .expect("Error loading posts");

    println!("Displaying {} messages", results.len());
    for message in results {
        println!("{}", message.message_id);
        println!("----------\n");
        println!("{}", message.body);
    }
}
