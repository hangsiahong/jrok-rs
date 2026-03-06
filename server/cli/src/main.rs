use clap::{Parser, Subcommand};
use colored::Colorize;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use tracing::info;

#[derive(Parser)]
#[command(name = "jrok")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Connect {
        #[arg(short, long)]
        port: u16,
        
        #[arg(short = 'H', long, default = "localhost")]
        host: String,
        
        #[arg(short, long)]
        subdomain: Option<String>,
        
        #[arg(short = 'P', long = default = "wss://tunnel.example.com/ws/agent")]
        server: String,
        
        #[arg(short = 'K', long, env = "JROK_API_KEY")]
        api_key: String,
        
        #[arg(long = "tcp")]
        tcp: bool,
    },
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Connect {
            port,
            host,
            subdomain,
            server,
            api_key,
            tcp,
        } => {
            connect_agent(port, host, subdomain, server, api_key, tcp).await
        }
    }
}

async fn connect_agent(
    port: u16,
    host: String,
    subdomain: Option<String>,
    server: String,
    api_key: String,
    is_tcp: bool,
) {
    let protocol = if is_tcp { "tcp" } else { "http" };
    let subdomain = subdomain.unwrap_or_else(|| {
        format!("jrok-{}", uuid::Uuid::new_v4().to_string().chars().take(8).collect())
    });
    
    println!("{}", format!("Connecting to {}...", server).green());
    
    let url = format!("{}/ws/agent?api_key={}", server, api_key);
    
    loop {
        match try_connect(&url, &subdomain, port, &host, protocol).await {
            Ok(tunnel_url) => {
                println!("{}", format!("✓ Tunnel active: {}", tunnel_url).green());
            }
            Err(e) => {
                println!("{}", format!("✗ Connection failed: {}", e).red());
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

async fn try_connect(
    url: &str,
    subdomain: &str,
    port: u16,
    host: &str,
    protocol: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let (ws, _) = connect_async(url).await?;
    
    let register_msg = serde_json::json!({
        "type": "register",
        "subdomain": subdomain,
        "local_port": port,
        "local_host": host,
        "protocol": protocol,
        "api_key": api_key,
    });
    
    ws.send(WsMessage::Text(register_msg)).await?;
    
    let mut ws = ws.filter_map(|msg| async {
        match msg {
            WsMessage::Text(text) => Some(text),
            _ => None,
        }
    });
    
    while let Some(msg) = ws.next().await {
        let response: serde_json::Value = serde_json::from_str(&msg)?;
        
        match response.get("type").and_then(|t| t.as_str()) {
            Some("welcome") => {
                let subdomain = response.get("subdomain").and_then(|s| s.as_str()).unwrap_or(subdomain);
                let protocol = response.get("protocol").and_then(|p| p.as_str()).unwrap_or("http");
                
                let url = match protocol {
                    "tcp" => format!("tcp://tunnel.example.com:{}", response.get("tcp_port").unwrap_or(0)),
                    _ => format!("https://{}.tunnel.example.com", subdomain),
                };
                
                return Ok(url);
            }
            Some("redirect") => {
                let new_server = response.get("server").and_then(|s| s.as_str()).unwrap_or("");
                return Err(format!("Redirect to: {}", new_server).into());
            }
            Some("error") => {
                let message = response.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown error");
                return Err(message.into());
            }
            _ => {}
        }
    }
    
    Err("Connection closed".into())
}
