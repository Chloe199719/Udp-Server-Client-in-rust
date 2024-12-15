use bytes::{BufMut, BytesMut};
use serde::{Deserialize, Serialize};

// Define an enum for message types.
#[derive(Debug, Clone, Copy)]
pub enum MessageType {
    PositionUpdate = 0x01,
    ChatMessage = 0x02,
    Heartbeat = 0x03,
    ConnectionInit = 0x04,
}

impl MessageType {
    pub fn from_byte(b: u8) -> Option<MessageType> {
        match b {
            0x01 => Some(MessageType::PositionUpdate),
            0x02 => Some(MessageType::ChatMessage),
            0x03 => Some(MessageType::Heartbeat),
            0x04 => Some(MessageType::ConnectionInit),
            _ => None,
        }
    }
}

// Example payloads:
#[derive(Debug, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Position {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Position { x, y, z }
    }
    pub fn serialize(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }
    pub fn deserialize(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Chat {
    pub text: String,
}

// Unified packet structure
// We'll store the payload as raw bytes. It's up to the caller
// to serialize/deserialize according to the message type.
#[derive(Debug)]
pub struct GamePacket {
    pub msg_type: MessageType,
    pub version: u8,
    pub seq_num: u32,
    pub payload: Vec<u8>,
}

impl GamePacket {
    pub fn new(msg_type: MessageType, seq_num: u32, payload: Vec<u8>) -> Self {
        GamePacket {
            msg_type,
            version: 1,
            seq_num,
            payload,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = BytesMut::with_capacity(1 + 1 + 4 + self.payload.len());
        buf.put_u8(self.msg_type as u8);
        buf.put_u8(self.version);
        buf.put_u32(self.seq_num);
        buf.put_slice(&self.payload);
        buf.to_vec()
    }

    pub fn deserialize(data: &[u8]) -> Option<GamePacket> {
        if data.len() < 6 {
            return None; // Not enough for header
        }
        let msg_type = MessageType::from_byte(data[0])?;
        let version = data[1];
        let seq_num = u32::from_be_bytes([data[2], data[3], data[4], data[5]]);
        let payload = data[6..].to_vec();
        Some(GamePacket {
            msg_type,
            seq_num,
            payload,
            version,
        })
    }
}
