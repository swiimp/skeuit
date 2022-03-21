use super::schema::messages;

#[derive(Queryable)]
pub struct Message {
    pub id: i32,
    pub message_id: String,
    pub body: String,
}

#[derive(Insertable)]
#[table_name="messages"]
pub struct NewMessage<'a> {
    pub message_id: &'a str,
    pub body: &'a str,
}
