use std::{
    collections::HashMap,
    io::{stdout, Write},
};

use bytes::{BufMut, BytesMut};
use crossterm::{
    cursor, execute,
    style::{self, Print},
    terminal::{self, Clear, ClearType},
};
use serde::{Deserialize, Serialize};
use std::time::Instant;

// Define an enum for message types.
#[derive(Debug, Clone, Copy)]
pub enum MessageType {
    PositionUpdate = 0x01,
    ChatMessage = 0x02,
    Heartbeat = 0x03,
    ConnectionInit = 0x04,
    PlayerJoin = 0x05,
    ConfirmPlayerMovement = 0x06,
    PlayerLeft = 0x07,
}

impl MessageType {
    pub fn from_byte(b: u8) -> Option<MessageType> {
        match b {
            0x01 => Some(MessageType::PositionUpdate),
            0x02 => Some(MessageType::ChatMessage),
            0x03 => Some(MessageType::Heartbeat),
            0x04 => Some(MessageType::ConnectionInit),
            0x05 => Some(MessageType::PlayerJoin),
            0x06 => Some(MessageType::ConfirmPlayerMovement),
            0x07 => Some(MessageType::PlayerLeft),
            _ => None,
        }
    }
}

// Example payloads:
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}
impl Position {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
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
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub position: Position,
    pub last_heartbeat: Instant,
    pub player_number: u32,
}

// Server state structure
#[derive(Debug, Clone)]
pub struct ServerState {
    pub players: HashMap<String, PlayerState>,
    pub board_size: (u32, u32),
}

impl ServerState {
    pub fn new(board_size: (u32, u32)) -> Self {
        ServerState {
            players: HashMap::new(),
            board_size,
        }
    }
    pub fn serialize(&self) -> Vec<u8> {
        //convert to ServerStateSend
        serde_json::to_vec(&ServerStateSend {
            players: self
                .players
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        PlayerStateSend {
                            position: v.position.clone(),
                        },
                    )
                })
                .collect(),
            board_size: self.board_size,
        })
        .unwrap()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]

pub struct ServerStateSend {
    pub players: HashMap<String, PlayerStateSend>,
    pub board_size: (u32, u32),
}
impl ServerStateSend {
    pub fn new() -> Self {
        ServerStateSend {
            players: HashMap::new(),
            board_size: (254, 254),
        }
    }
    pub fn deserialize(data: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(data)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerStateSend {
    pub position: Position,
}

impl PlayerStateSend {
    pub fn new() -> Self {
        PlayerStateSend {
            position: Position::new(0, 0, 0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerUpdate {
    pub player: String,
    pub position: Position,
}

impl PlayerUpdate {
    pub fn serialize(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }
    pub fn deserialize(data: &[u8]) -> Option<Self> {
        serde_json::from_slice(data).ok()
    }
}
// const BOARD_WIDTH: u32 = 254;
// const BOARD_HEIGHT: u32 = 254;
pub fn render_board(players: &HashMap<String, PlayerState>) -> Result<(), std::io::Error> {
    let mut stdout = stdout();

    // Clear the terminal and hide the cursor
    execute!(stdout, Clear(ClearType::All), cursor::Hide)?;

    // Get terminal size
    let (term_width, term_height) = terminal::size()?;
    let center_x = (term_width / 2) as i32;
    let center_y = (term_height / 2) as i32;

    // Draw dynamic board borders based on terminal size
    for y in 0..term_height {
        for x in 0..term_width {
            // Draw horizontal borders
            if y == 0 || y == term_height - 1 {
                execute!(stdout, cursor::MoveTo(x, y), Print("#"))?;
            }
            // Draw vertical borders
            if x == 0 || x == term_width - 1 {
                execute!(stdout, cursor::MoveTo(x, y), Print("#"))?;
            }
        }
    }

    // Render players
    for (_addr, player) in players {
        let pos = &player.position;

        // Convert logical position to screen coordinates
        let screen_x = center_x + pos.x as i32;
        let screen_y = center_y - pos.y as i32;

        // Ensure the player's position is visible
        if screen_x >= 0
            && screen_x < term_width as i32
            && screen_y >= 0
            && screen_y < term_height as i32
        {
            execute!(
                stdout,
                cursor::MoveTo(screen_x as u16, screen_y as u16),
                style::SetForegroundColor(style::Color::Green),
                Print(format!("P{}", player.player_number)), // Represent player with 'P'
                style::ResetColor
            )?;
        }
    }

    // Move cursor out of the way
    execute!(stdout, cursor::MoveTo(0, term_height))?;

    stdout.flush()?; // Ensure everything is drawn to the screen
    Ok(())
}
