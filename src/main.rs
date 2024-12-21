use crossterm::terminal;
use game_udp::{Chat, GamePacket, MessageType, PlayerState, PlayerUpdate, Position, ServerState};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    net::UdpSocket,
    sync::Mutex,
    task,
    time::{self},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "0.0.0.0:4000";
    let socket = Arc::new(UdpSocket::bind(server_addr).await?);
    println!("Server listening on {}", server_addr);
    let size = terminal::size().unwrap();

    let state = Arc::new(Mutex::new(ServerState::new((size.0 as u32, size.1 as u32))));

    // Start a task for cleaning up disconnected players
    let cleanup_state = Arc::clone(&state);
    let cleanup_socket = Arc::clone(&socket);
    task::spawn(async move {
        let interval = time::interval(Duration::from_secs(5));
        tokio::pin!(interval);

        loop {
            interval.tick().await;
            let mut state = cleanup_state.lock().await;
            let now = Instant::now();
            let ids_to_remove: Vec<String> = state
                .players
                .iter()
                .filter_map(|(addr, player)| {
                    if now.duration_since(player.last_heartbeat) > Duration::from_secs(10) {
                        // println!("Removing inactive player: {}", addr);
                        Some(addr.clone())
                    } else {
                        None
                    }
                })
                .collect();
            for id in ids_to_remove {
                let packet = GamePacket::new(MessageType::PlayerLeft, 0, id.as_bytes().to_vec());
                let data = packet.serialize();
                for (addr, _) in &state.players {
                    if addr != &id {
                        cleanup_socket.send_to(&data, addr).await.unwrap();
                    }
                }
            }
            state.players.retain(|_addr, player| {
                if now.duration_since(player.last_heartbeat) > Duration::from_secs(10) {
                    // println!("Removing inactive player: {}", addr);
                    false
                } else {
                    true
                }
            });
            game_udp::render_board(&state.players).unwrap();
        }
    });
    // Start a Task to ping all players
    // Start a task for sending heartbeats
    let ping_socket = Arc::clone(&socket);
    let ping_state = Arc::clone(&state);
    task::spawn(async move {
        let interval = time::interval(Duration::from_secs(3));
        tokio::pin!(interval);

        loop {
            interval.tick().await;
            let state = ping_state.lock().await;
            for (addr, _) in &state.players {
                let reply = GamePacket::new(MessageType::Heartbeat, 0, vec![]);
                let data = reply.serialize();
                if let Ok(addr) = addr.parse::<std::net::SocketAddr>() {
                    if let Err(e) = ping_socket.send_to(&data, addr).await {
                        eprintln!("Failed to send heartbeat to {}: {}", addr, e);
                    }
                }
            }
        }
    });
    let mut buf = vec![0u8; 1500];
    let mut player_number = 0;
    loop {
        let (len, client_addr) = socket.recv_from(&mut buf).await?;
        let client_addr_str = client_addr.to_string();

        if let Some(packet) = GamePacket::deserialize(&buf[..len]) {
            // println!("Received {:?} from {}", packet, client_addr);

            match packet.msg_type {
                MessageType::PositionUpdate => {
                    let position = Position::deserialize(&packet.payload).unwrap();
                    let mut state = state.lock().await;
                    let current_player_position = state
                        .players
                        .get(&client_addr_str)
                        .unwrap() // FIXME: Handle error
                        .position
                        .clone();
                    if position.x < -(state.board_size.0 as i32) / 2
                        || position.x >= (state.board_size.0 as i32) / 2
                    {
                        // Invalid move, reset player position
                        let player_packet = GamePacket::new(
                            MessageType::ConfirmPlayerMovement,
                            packet.seq_num,
                            current_player_position.serialize(),
                        );
                        let data = player_packet.serialize();
                        socket.send_to(&data, &client_addr).await?;
                        continue;
                    }
                    if position.y - 2 < -(state.board_size.1 as i32) / 2
                        || position.y >= (state.board_size.1 as i32) / 2
                    {
                        // Invalid move, reset player position
                        let player_packet = GamePacket::new(
                            MessageType::ConfirmPlayerMovement,
                            packet.seq_num,
                            current_player_position.serialize(),
                        );
                        let data = player_packet.serialize();
                        socket.send_to(&data, &client_addr).await?;
                        continue;
                    }
                    // Update player position
                    if let Some(player) = state.players.get_mut(&client_addr_str) {
                        player.position = position.clone();
                        player.last_heartbeat = Instant::now();
                    } else {
                        // New player connecting
                        state.players.insert(
                            client_addr_str.clone(),
                            PlayerState {
                                position: position.clone(),
                                last_heartbeat: Instant::now(),
                                player_number,
                            },
                        );
                        player_number += 1;
                    }

                    // Notify all players about the move
                    let update_packet = GamePacket::new(
                        MessageType::PositionUpdate,
                        packet.seq_num,
                        PlayerUpdate {
                            player: client_addr_str.clone(),
                            position: position.clone(),
                        }
                        .serialize(),
                    );
                    let data = update_packet.serialize();
                    for (addr, _) in &state.players {
                        if addr != &client_addr_str {
                            socket.send_to(&data, addr).await?;
                        } else {
                            let player_packet = GamePacket::new(
                                MessageType::ConfirmPlayerMovement,
                                packet.seq_num,
                                position.serialize(),
                            );
                            let data = player_packet.serialize();
                            socket.send_to(&data, addr).await?;
                        }
                    }
                    game_udp::render_board(&state.players).unwrap();
                }
                MessageType::ChatMessage => {
                    if let Ok(chat) = serde_json::from_slice::<Chat>(&packet.payload) {
                        // println!("Player says: {}", chat.text);

                        // Broadcast chat to all players
                        let chat_packet = GamePacket::new(
                            MessageType::ChatMessage,
                            packet.seq_num,
                            serde_json::to_vec(&chat).unwrap(),
                        );
                        let data = chat_packet.serialize();
                        let state = state.lock().await;
                        for (addr, _) in &state.players {
                            socket.send_to(&data, addr).await?;
                        }
                    }
                }
                MessageType::Heartbeat => {
                    // Update heartbeat
                    let mut state = state.lock().await;
                    if let Some(player) = state.players.get_mut(&client_addr_str) {
                        player.last_heartbeat = Instant::now();
                    }
                }
                MessageType::ConnectionInit => {
                    // Send current state to new player
                    let mut state = state.lock().await;

                    state.players.insert(
                        client_addr_str.clone(),
                        PlayerState {
                            position: Position { x: 0, y: 0, z: 0 },
                            last_heartbeat: Instant::now(),
                            player_number,
                        },
                    );
                    player_number += 1;
                    let current_state = state.clone();
                    drop(state);
                    let reply = GamePacket::new(
                        MessageType::ConnectionInit,
                        packet.seq_num,
                        current_state.serialize(),
                    );
                    let data = reply.serialize();
                    socket.send_to(&data, &client_addr).await?;

                    // Notify all players about the new player
                    let new_player = GamePacket::new(
                        MessageType::PlayerJoin,
                        packet.seq_num,
                        client_addr_str.clone().as_bytes().to_vec(),
                    );
                    let data = new_player.serialize();
                    for (addr, _) in &current_state.players {
                        if addr != &client_addr_str {
                            socket.send_to(&data, addr).await?;
                        }
                    }

                    // Send welcome message

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
                _ => {}
            }
        }
    }
}
