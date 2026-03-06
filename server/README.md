# jrok - Distributed Tunnel Server

A high-performance, distributed tunnel server written in Rust that allows you to expose local services behind NAT/firewalls to the public internet.

## 🚀 Features

- **HTTP Tunneling** - Expose local HTTP/HTTPS servers to the world
- **TCP Tunneling** - Forward any TCP traffic (SSH, databases, etc.) using rexpose
- **Multi-Server Cluster** - Distributed architecture with automatic leader election
- **Secure Authentication** - API key-based agent authentication
- **Automatic Failover** - Cross-server redirection for high availability
- **Built with Rust** - Memory-safe, blazing fast, and reliable
- **Zero Downtime** - Leader election means no single point of failure

## 📋 Quick Start

### Prerequisites

- Rust 1.70+ (for building from source)
- Turso database account (free tier works)
- Domain name with DNS configured (optional but recommended)

### 1. Set Up Database

```bash
# Install Turso CLI
curl -sSfL https://get.tur.so/install.sh | bash

# Create database
turso db create jrok

# Get your credentials
turso db show jrok --url    # Save this as TURSO_URL
turso db tokens create jrok  # Save this as TURSO_TOKEN

# Run migrations
turso db shell jrok < migrations/001_init.sql
```

### 2. Configure Environment

Create a `.env` file:

```bash
# Server Configuration
SERVER_ID=server-1
HTTP_HOST=0.0.0.0
HTTP_PORT=8080
TCP_HOST=0.0.0.0

# Database
TURSO_URL=libsql://your-db.turso.io
TURSO_TOKEN=your-auth-token

# Timing (milliseconds)
HEARTBEAT_INTERVAL_MS=5000
LEADER_TIMEOUT_MS=15000
AGENT_TIMEOUT_MS=30000
```

### 3. Create API Key

```bash
# Using Turso CLI (simplest method)
turso db shell $TURSO_URL <<SQL
INSERT INTO api_keys (id, key_hash, key_prefix, name, created_at)
VALUES (
  '$(uuidgen | tr -d '-')',
  '$(echo -n "jrok_prod_$(uuidgen | tr -d '-')" | openssl dgst -sha256 -hmac "jrok-api-key-v1" -binary | base64)',
  'prod',
  'Production Agent',
  $(date +%s)000
);
SQL

# Save the displayed key - you won't see it again!
```

See [API_KEYS.md](API_KEYS.md) for more API key management options.

### 4. Build and Run

```bash
# Build release binary
cargo build --release

# Run server
./target/release/jrok-server
```

Server will start on `http://0.0.0.0:8080`

## 📖 Usage

### HTTP Tunneling

Expose a local web server:

```bash
# 1. Start your local service
cd /path/to/your/app
python3 -m http.server 3000

# 2. Register agent with jrok (using websocat)
cargo install websocat
websocat -c ws://localhost:8080/ws/agent

# Paste this message:
{"type":"register","subdomain":"myapp","local_port":3000,"local_host":"localhost","protocol":"http","api_key":"jrok_YOUR_KEY"}

# 3. Access your app via tunnel
curl http://localhost:8080/myapp/
```

### TCP Tunneling

Forward SSH or database connections:

```bash
# 1. Register TCP agent
websocat -c ws://localhost:8080/ws/agent

# Paste this message:
{"type":"register","subdomain":"ssh","local_port":22,"local_host":"localhost","protocol":"tcp","api_key":"jrok_YOUR_KEY"}

# 2. Connect to the tunnel
# The server will allocate a TCP port (starting at 10000)
nc localhost 10000  # Forwards to localhost:22 on your machine
```

### Multi-Server Setup

Run multiple jrok servers for high availability:

```bash
# Server 1
SERVER_ID=server-1 HTTP_PORT=8080 ./target/release/jrok-server

# Server 2
SERVER_ID=server-2 HTTP_PORT=8081 ./target/release/jrok-server

# Server 3
SERVER_ID=server-3 HTTP_PORT=8082 ./target/release/jrok-server
```

All servers share the same database and coordinate automatically. One becomes leader, others redirect as needed.

### Using with Cloudflare Load Balancer

For production deployment with automatic SSL:

```bash
# 1. Deploy 3+ servers
# 2. Create Cloudflare Load Balancer
# 3. Add origins: http://server-1.your-domain.com:8080
#                  http://server-2.your-domain.com:8081
#                  http://server-3.your-domain.com:8082
# 4. Enable health checks on /health
# 5. Add your domain
# 6. SSL handled automatically!
```

## 🔧 Configuration

### Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `SERVER_ID` | Unique server identifier | Auto-generated UUID | No |
| `HTTP_HOST` | HTTP bind address | `0.0.0.0` | Yes |
| `HTTP_PORT` | HTTP port | `8080` | No |
| `TCP_HOST` | TCP bind address | `0.0.0.0` | Yes |
| `TURSO_URL` | Turso database URL | - | Yes |
| `TURSO_TOKEN` | Turso auth token | - | Yes |
| `HEARTBEAT_INTERVAL_MS` | Heartbeat frequency | `5000` | No |
| `LEADER_TIMEOUT_MS` | Leader election timeout | `15000` | No |
| `AGENT_TIMEOUT_MS` | Agent timeout | `30000` | No |

### Protocol Types

- **http** - HTTP/HTTPS tunneling
- **tcp** - Raw TCP forwarding (powered by rexpose)

## 🏗️ Architecture

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│   Client    │────────▶│  jrok       │────────▶│   Agent     │
│             │◀────────│  Server     │◀────────│             │
└─────────────┘         └─────────────┘         └─────────────┘
                              │
                              ▼
                        ┌─────────────┐
                        │  Turso DB   │
                        │  (libsql)   │
                        └─────────────┘
```

**Components:**

- **Server** - Main HTTP/WebSocket server handling incoming connections
- **Agent** - Runs on your local machine, connects to server via WebSocket
- **Database** - Shared state for multi-server coordination
- **Cluster** - Leader election and health monitoring

## 📊 Monitoring

### Health Check

```bash
curl http://localhost:8080/health
# Returns: ok
```

### Agent Status

```bash
# Query database for active agents
turso db shell $TURSO_URL <<SQL
SELECT subdomain, server_id, protocol, local_host, local_port,
       datetime(last_heartbeat/1000, 'unixepoch') as last_seen
FROM agents
WHERE active = 1
ORDER BY last_heartbeat DESC;
SQL
```

### Server Cluster Status

```bash
# View all servers and their health
turso db shell $TURSO_URL <<SQL
SELECT id, http_host, is_healthy,
       datetime(last_heartbeat/1000, 'unixepoch') as last_heartbeat
FROM servers;
SQL
```

## 🐛 Troubleshooting

### Agent Cannot Connect

1. Check API key is valid
2. Verify server is reachable (`curl http://localhost:8080/health`)
3. Check firewall allows WebSocket connections
4. Review server logs for error messages

### Tunnel Returns 404

1. Verify agent is registered with correct subdomain
2. Check agent heartbeat is recent
3. Ensure agent supports the requested protocol (http/tcp)

### TCP Tunnel Not Working

1. Confirm agent registered with `protocol: "tcp"`
2. Check local service is listening on specified port
3. Verify no firewall blocking local connection
4. Check server logs for TCP listener status

### High Availability Issues

1. Verify all servers can reach the database
2. Check `LEADER_TIMEOUT_MS` is appropriate for your network
3. Monitor server health in database
4. Check logs for leadership changes

## 🔨 Development

### Build from Source

```bash
# Clone repository
git clone https://github.com/your-org/jrok-rs.git
cd jrok-rs/server

# Build release binary
cargo build --release

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug ./target/release/jrok-server
```

### Running Tests

```bash
# Run all tests
cargo test

# Run integration tests
cargo test --test integration_test

# Run specific test
cargo test test_agent_registration
```

### Project Structure

```
server/
├── src/
│   ├── agent/      # WebSocket agent handling
│   ├── api/        # HTTP API endpoints (CLI-based key management)
│   ├── cluster/    # Multi-server coordination
│   ├── db/         # Database layer with Turso/libsql
│   ├── tunnel/     # HTTP tunnel proxy logic
│   ├── tcp/        # TCP tunneling with rexpose integration
│   ├── proto.rs    # WebSocket message protocol
│   ├── error.rs    # Error types
│   ├── config.rs   # Configuration management
│   └── main.rs     # Server entry point
├── tests/          # Integration tests
├── migrations/     # Database migrations
├── API_KEYS.md     # API key management guide
└── Cargo.toml      # Dependencies
```

## 🔐 Security

### API Key Management

API keys are hashed with HMAC-SHA256 before storage. The actual keys are never stored in the database.

**Best Practices:**
- Generate unique keys for each agent
- Rotate keys regularly
- Use descriptive key names for tracking
- Never commit keys to version control
- Delete keys for decommissioned agents

See [API_KEYS.md](API_KEYS.md) for detailed key management procedures.

### Network Security

- All WebSocket connections require valid API keys
- Cross-server redirection validates server identity
- Database connections use encrypted TLS
- No plaintext credentials stored anywhere

## 📝 License

MIT License - see LICENSE file for details

## 🤝 Contributing

Contributions welcome! Please feel free to submit a Pull Request.

**Development Guidelines:**
1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a PR

## 🙏 Acknowledgments

Built with amazing Rust libraries:
- [Tokio](https://tokio.rs/) - Async runtime
- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [Turso](https://turso.tech/) - Edge SQLite database
- [rexpose](https://github.com/k4i6/rexpose) - TCP tunneling
- [WebSocket](https://github.com/snapview/tokio-tungstenite) - WebSocket support

## 📧 Support

- **Issues**: [GitHub Issues](https://github.com/your-org/jrok-rs/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-org/jrok-rs/discussions)
- **Documentation**: [See Wiki](https://github.com/your-org/jrok-rs/wiki)

---

**Need help?** Open an issue or join our community discussions!

**Status**: ✅ Production Ready - HTTP Tunneling | 🚧 TCP Tunneling (Beta)
