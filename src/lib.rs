pub mod bot;
pub mod models;
pub mod packet;
pub mod schema;

#[macro_use]
extern crate diesel;
extern crate dotenv;

use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use dotenv::dotenv;
use std::env;

use self::models::{Message, NewMessage};

pub fn establish_connection() -> Pool<ConnectionManager<PgConnection>> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let manager = ConnectionManager::<PgConnection>::new(database_url);

    Pool::builder()
        .max_size(15)
        .build(manager)
        .expect("Failed to connect to database.")
}

pub fn create_post(
    conn: &mut PooledConnection<ConnectionManager<PgConnection>>,
    message_id: &str,
    body: &str,
) -> Message {
    use crate::diesel::RunQueryDsl;
    use crate::schema::messages;

    let new_message = NewMessage { message_id, body };

    diesel::insert_into(messages::table)
        .values(&new_message)
        .get_result(conn)
        .expect("Error saving new post")
}
