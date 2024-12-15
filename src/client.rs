use std::net::SocketAddr;

use game_udp::{Chat, GamePacket, MessageType, Position};
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr: SocketAddr = "127.0.0.1:4000".parse()?;
    let client_addr = "0.0.0.0:0"; // OS chooses a free port
    let socket = UdpSocket::bind(client_addr).await?;
    socket.connect(&server_addr).await?;

    let mut sequence_num = 1;
    let init_packet = GamePacket::new(MessageType::ConnectionInit, sequence_num, vec![]);
    sequence_num += 1;
    socket.send(&init_packet.serialize()).await?;
    // 1. Send a PositionUpdate
    let position = Position {
        x: 10.0,
        y: 5.0,
        z: 3.0,
    };
    let position_bytes = serde_json::to_vec(&position)?;
    let position_packet =
        GamePacket::new(MessageType::PositionUpdate, sequence_num, position_bytes);
    sequence_num += 1;
    socket.send(&position_packet.serialize()).await?;

    // 2. Send a ChatMessage
    let chat = Chat {
        text: "Hello, world!".into(),
    };
    let chat_bytes = serde_json::to_vec(&chat)?;
    let chat_packet = GamePacket::new(MessageType::ChatMessage, sequence_num, chat_bytes);
    sequence_num += 1;
    socket.send(&chat_packet.serialize()).await?;

    // 3. Send a Heartbeat
    let hb_packet = GamePacket::new(MessageType::Heartbeat, sequence_num, vec![]);
    sequence_num += 1;
    socket.send(&hb_packet.serialize()).await?;

    // Listen for a response
    let mut buf = vec![0u8; 1500];
    if let Ok(len) = socket.recv(&mut buf).await {
        if let Some(reply) = GamePacket::deserialize(&buf[..len]) {
            println!("Received reply from server: {:?}", reply);
        }
    }

    Ok(())
}
