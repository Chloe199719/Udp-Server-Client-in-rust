use game_udp::{Chat, GamePacket, MessageType, Position};
use std::net::SocketAddr;
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "0.0.0.0:4000";
    let socket = UdpSocket::bind(server_addr).await?;
    println!("Server listening on {}", server_addr);

    let mut buf = vec![0u8; 1500];

    loop {
        let (len, client_addr) = socket.recv_from(&mut buf).await?;
        if let Some(packet) = GamePacket::deserialize(&buf[..len]) {
            println!("Received {:?} from {}", packet, client_addr);

            // Handle packet
            match packet.msg_type {
                MessageType::PositionUpdate => {
                    // Maybe update server state with player position
                    // For demonstration, we do nothing special here
                    let position = Position::deserialize(&packet.payload).unwrap();
                    println!("Player position: {:?}", position);
                }
                MessageType::ChatMessage => {
                    // Deserialize the chat message
                    if let Ok(chat) = serde_json::from_slice::<Chat>(&packet.payload) {
                        println!("Player says: {}", chat.text);
                    }
                }
                MessageType::Heartbeat => {
                    // Just send a heartbeat back as an acknowledgment
                    let reply = GamePacket::new(MessageType::Heartbeat, packet.seq_num, vec![]);
                    let data = reply.serialize();
                    socket.send_to(&data, &client_addr).await?;
                }
                MessageType::ConnectionInit => {
                    // Send a welcome message
                    let welcome = Chat {
                        text: "Welcome to the server!".to_string(),
                    };
                    let reply = GamePacket::new(
                        MessageType::ChatMessage,
                        packet.seq_num,
                        serde_json::to_vec(&welcome).unwrap(),
                    );
                    let data = reply.serialize();
                    socket.send_to(&data, &client_addr).await?;
                }
            }
        }
    }
}
