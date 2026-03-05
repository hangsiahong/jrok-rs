# Jrok-RS

High-performance, distributed tunnel service in Rust.

## Features

- **Distributed**: Multiple servers with automatic failover
- **No SPOF**: Any server can become leader
- **Fast**: Sub-millisecond routing
- **Simple**: Single binary, minimal config
- **TCP + HTTP**: Supports both protocols
- **Auto SSL**: Cloudflare LB handles all certificates

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                                                         │
│   Client ──HTTPS──▶ Cloudflare LB ──HTTP──▶ Servers    │
│            (SSL)                        (3+ servers)    │
│                                                         │
│                                      ┌─────────────┐   │
│                                      │   Turso DB  │   │
│                                      └─────────────┘   │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Setup Turso

```bash
turso db create jrok
turso db shell jrok < migrations/001_init.sql
```

### 2. Start Server

```bash
cp .env.example .env
# Edit .env with your values
cargo run --release
```

### 3. Create API Key

```bash
curl -X POST http://localhost:8080/api/keys \
  -H "Content-Type: application/json" \
  -d '{"name": "my-key"}'
```

### 4. Connect Agent

```bash
cargo run --package jrok-cli -- connect --port 3000 --api-key YOUR_KEY
```

## Configuration

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for full configuration.

## Deployment

### With Cloudflare Load Balancer

1. Deploy 3+ servers on different hosts
2. Create Cloudflare LB with origins pointing to your servers
3. Enable health checks on `/health`
4. SSL handled automatically by Cloudflare

### Single Server

Just run the binary. Cloudflare can still provide SSL.

## Development

```bash
cargo build
cargo test
cargo run
```

## License

MIT
