use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use game_udp::{
    Chat, GamePacket, MessageType, PlayerStateSend, PlayerUpdate, Position, ServerStateSend,
};
use tokio::{
    net::UdpSocket,
    sync::Mutex,
    time::{Duration, Instant},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr: SocketAddr = "127.0.0.1:4000".parse()?;
    let client_addr = "0.0.0.0:0"; // OS chooses a free port

    let socket = UdpSocket::bind(client_addr).await?;
    socket.connect(&server_addr).await?;
    let socket = Arc::new(socket);

    let sequence_num = Arc::new(Mutex::new(1u32));
    let shutdown_signal = Arc::new(AtomicBool::new(false));

    // Initialize connection
    {
        let mut seq = sequence_num.lock().await;
        let init_packet = GamePacket::new(MessageType::ConnectionInit, *seq, vec![]);
        *seq += 1;
        socket.send(&init_packet.serialize()).await?;
    }
    let server_state = Arc::new(Mutex::new(ServerStateSend::new()));
    // Shared position state
    let position = Arc::new(Mutex::new(Position { x: 0, y: 0, z: 0 }));

    // Task for handling incoming messages
    {
        let socket = Arc::clone(&socket);
        let sequence_num = Arc::clone(&sequence_num);
        let shutdown_signal = Arc::clone(&shutdown_signal);
        let position = Arc::clone(&position);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 1500];
            while !shutdown_signal.load(Ordering::Relaxed) {
                if let Ok(len) = socket.recv(&mut buf).await {
                    if let Some(reply) = GamePacket::deserialize(&buf[..len]) {
                        match reply.msg_type {
                            MessageType::Heartbeat => {
                                let mut seq = sequence_num.lock().await;
                                let hb_packet =
                                    GamePacket::new(MessageType::Heartbeat, *seq, vec![]);
                                *seq += 1;
                                if let Err(e) = socket.send(&hb_packet.serialize()).await {
                                    eprintln!("Failed to send heartbeat response: {}", e);
                                }
                            }
                            MessageType::PositionUpdate => {
                                let player_state = PlayerUpdate::deserialize(&reply.payload);
                                if let Some(player_state) = player_state {
                                    let mut state = server_state.lock().await;
                                    if let Some(player) =
                                        state.players.get_mut(&player_state.player)
                                    {
                                        player.position = player_state.position;
                                    }

                                    // println!("Server PositionUpdate: {:?}", state);
                                }
                            }
                            MessageType::ChatMessage => {
                                // println!("Server ChatMessage: {:?}", reply);
                            }
                            MessageType::ConnectionInit => {
                                let server_state_deralized =
                                    ServerStateSend::deserialize(&reply.payload);
                                if let Ok(server_state_deralized) = server_state_deralized {
                                    let mut state = server_state.lock().await;
                                    *state = server_state_deralized;
                                }
                            }
                            MessageType::PlayerJoin => {
                                let player = String::from_utf8(reply.payload).unwrap();
                                let mut state = server_state.lock().await;
                                state.players.insert(player, PlayerStateSend::new());
                            }
                            MessageType::ConfirmPlayerMovement => {
                                let player_state = Position::deserialize(&reply.payload);
                                let mut position2 = position.lock().await;
                                *position2 = player_state.unwrap();
                            }
                        }
                    }
                }
            }
        });
    }

    // Task for reading user input and sending position updates or chat messages
    {
        let socket = Arc::clone(&socket);
        let sequence_num = Arc::clone(&sequence_num);
        let position = Arc::clone(&position);
        let shutdown_signal = Arc::clone(&shutdown_signal);
        tokio::spawn(async move {
            enable_raw_mode().expect("Failed to enable raw mode");
            println!(
                "Use 'w', 'a', 's', 'd' to move position. Press 'c' followed by your message to send a chat message. Press 'q' to quit."
            );

            let mut chat_mode = false;
            let mut chat_message = String::new();
            let mut last_position_update = Instant::now();
            let position_update_cooldown = Duration::from_millis(100);

            loop {
                if event::poll(std::time::Duration::from_millis(100)).unwrap() {
                    if let Event::Key(key_event) = event::read().unwrap() {
                        match key_event.code {
                            KeyCode::Char('q') => {
                                println!("Exiting...");
                                shutdown_signal.store(true, Ordering::Relaxed);
                                break;
                            }
                            KeyCode::Char('c') if !chat_mode => {
                                chat_mode = true;
                                chat_message.clear();
                                println!("Enter chat message: ");
                            }
                            KeyCode::Char(c) if chat_mode => {
                                if c == '\n' {
                                    chat_mode = false;
                                    let chat = Chat {
                                        text: chat_message.clone(),
                                    };
                                    let chat_bytes = serde_json::to_vec(&chat).unwrap();

                                    let mut seq = sequence_num.lock().await;
                                    let chat_packet =
                                        GamePacket::new(MessageType::ChatMessage, *seq, chat_bytes);
                                    *seq += 1;

                                    if let Err(e) = socket.send(&chat_packet.serialize()).await {
                                        eprintln!("Failed to send chat message: {}", e);
                                    }
                                    chat_message.clear();
                                } else {
                                    chat_message.push(c);
                                }
                            }
                            KeyCode::Char(c) if !chat_mode => {
                                if last_position_update.elapsed() >= position_update_cooldown {
                                    let position_bytes = {
                                        let mut pos = position.lock().await;
                                        match c {
                                            'w' => pos.y += 1,
                                            's' => pos.y -= 1,
                                            'a' => pos.x -= 1,
                                            'd' => pos.x += 1,
                                            _ => {
                                                println!("Unknown command: {}", c);
                                                continue;
                                            }
                                        }

                                        serde_json::to_vec(&*pos).unwrap()
                                    };

                                    let position_packet = {
                                        let mut seq = sequence_num.lock().await;
                                        let position_packet = GamePacket::new(
                                            MessageType::PositionUpdate,
                                            *seq,
                                            position_bytes,
                                        );
                                        *seq += 1;
                                        position_packet
                                    };

                                    if let Err(e) = socket.send(&position_packet.serialize()).await
                                    {
                                        eprintln!("Failed to send position update: {}", e);
                                    }

                                    last_position_update = Instant::now();
                                } else {
                                    // println!("Position update cooldown active.");
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            disable_raw_mode().expect("Failed to disable raw mode");
        });
    }

    // Keep the main task alive until shutdown signal is triggered.
    while !shutdown_signal.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    println!("Main thread shutting down.");
    Ok(())
}
