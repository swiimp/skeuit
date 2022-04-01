use serde_json::Value;
use std::io::Error;

#[derive(Clone)]
pub struct Packet {
    pub op: u64,
    pub d: String,
    pub t: String,
    pub s: u64,
}

impl Packet {
    pub fn new(op_code: u64, data: String, transaction_type: String, seq_num: u64) -> Packet {
        Packet {
            op: op_code,
            d: data,
            t: transaction_type,
            s: seq_num,
        }
    }

    pub fn from(packet_data: String) -> Result<Packet, Error> {
        let json: Value = serde_json::from_str(&packet_data).unwrap();
        let mut seq_num: u64 = 0;
        if serde_json::to_string(&json["s"]).unwrap() != "null" {
            seq_num = serde_json::to_string(&json["s"])
                .unwrap()
                .parse::<u64>()
                .unwrap();
        }
        Ok(Packet {
            op: serde_json::to_string(&json["op"])
                .unwrap()
                .parse::<u64>()
                .unwrap(),
            d: serde_json::to_string(&json["d"]).unwrap(),
            t: serde_json::to_string(&json["t"]).unwrap(),
            s: seq_num,
        })
    }

    pub fn to_string(&self) -> String {
        let result: String;
        if self.op != 0 {
            result = format!("\"op\":{},\"d\":{},\"t\":null,\"s\":null", self.op, self.d);
        } else {
            result = format!(
                "\"op\":{},\"d\":{},\"t\":{},\"s\":{}",
                self.op, self.d, self.t, self.s
            );
        }
        "{".to_owned() + &result + "}"
    }
}
