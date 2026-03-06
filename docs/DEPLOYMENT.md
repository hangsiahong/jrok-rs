# Deployment Guide

## Quick Start

### 1. Setup Turso

```bash
# Install Turso CLI
curl -sSfL https://get.tur.so/install.sh | bash

# Login
turso auth login

# Create database
turso db create jrok

# Get connection info
turso db show jrok

# Run migrations
turso db shell jrok < migrations/001_init.sql

# Create a token
turso db tokens create jrok
```

### 2. Build Server

```bash
cargo build --release
```

### 3. Run Single Server

```bash
# Create .env file
cat > .env << EOF
SERVER_ID=server-1
HTTP_HOST=tunnel1.example.com
HTTP_PORT=8080
TCP_HOST=1.2.3.4
BASE_DOMAIN=tunnel.example.com
TURSO_URL=https://your-db.turso.io
TURSO_TOKEN=your-token-here
EOF

# Run
./target/release/jrok-server
```

## Cloudflare Setup

### Option A: Cloudflare Load Balancer (Recommended)

1. **Add DNS Records**

```
# For each server, create A record
tunnel1.example.com  A  1.2.3.4
tunnel2.example.com  A  5.6.7.8
tunnel3.example.com  A  9.10.11.12

# Wildcard for subdomains (CNAME to LB)
*.tunnel.example.com  CNAME  tunnel-lb.example.com
```

2. **Create Load Balancer**

```
Name: tunnel-lb
Pool:
  - tunnel1.example.com:8080
  - tunnel2.example.com:8080
  - tunnel3.example.com:8080

Health Check:
  - Path: /health
  - Interval: 5s
  - Timeout: 3s

Fallback: None (let Cloudflare handle)
```

3. **Configure DNS for LB**

```
tunnel-lb.example.com  LB  (point to your load balancer)
```

**Cost:** $5/month (1 LB) + $1.50/month (3 origins)

### Option B: DNS Round Robin (Free)

1. **Add DNS Records**

```
# Multiple A records for same hostname
tunnel.example.com  A  1.2.3.4
tunnel.example.com  A  5.6.7.8
tunnel.example.com  A  9.10.11.12

# Wildcard CNAME
*.tunnel.example.com  CNAME  tunnel.example.com
```

**Cost:** $0

**Downside:** No health checks, clients might hit dead server

### Option C: Client-Side Failover (Free + Reliable)

Modify CLI to try multiple servers:

```rust
let servers = vec![
    "https://tunnel1.example.com",
    "https://tunnel2.example.com",
    "https://tunnel3.example.com",
];

for server in servers {
    if let Ok(conn) = try_connect(server).await {
        return Ok(conn);
    }
}
```

## SSL Configuration

### With Cloudflare (Recommended)

Cloudflare provides free SSL. Your servers run HTTP only.

```
Client ──HTTPS──▶ Cloudflare ──HTTP──▶ Your Server
        (SSL here)              (no SSL needed)
```

**Server Config:**
- Bind to HTTP port (e.g., 8080)
- No TLS certificates needed
- Cloudflare terminates SSL

### Without Cloudflare (Self-Managed SSL)

If you want to handle SSL yourself:

1. **Install ACME Client**

```bash
apt install certbot
```

2. **Get Certificate**

```bash
certbot certonly --standalone -d tunnel1.example.com
```

3. **Modify Server Config**

```rust
// Add TLS support to server
let cert = tokio::fs::read("/etc/letsencrypt/live/tunnel1.example.com/fullchain.pem").await?;
let key = tokio::fs::read("/etc/letsencrypt/live/tunnel1.example.com/privkey.pem").await?;
```

**Not recommended** - adds complexity. Use Cloudflare instead.

## Multiple Server Deployment

### 1. Prepare Each Server

```bash
# Server 1
SERVER_ID=server-1 \
HTTP_HOST=tunnel1.example.com \
TCP_HOST=1.2.3.4 \
./jrok-server

# Server 2
SERVER_ID=server-2 \
HTTP_HOST=tunnel2.example.com \
TCP_HOST=5.6.7.8 \
./jrok-server

# Server 3
SERVER_ID=server-3 \
HTTP_HOST=tunnel3.example.com \
TCP_HOST=9.10.11.12 \
./jrok-server
```

### 2. Verify Cluster

```bash
# Check each server
curl http://tunnel1.example.com:8080/health
curl http://tunnel2.example.com:8080/health
curl http://tunnel3.example.com:8080/health

# All should return: {"status":"ok"}
```

### 3. Test Failover

```bash
# Connect agent
jrok connect --port 3000

# Kill server where agent connected
# Agent should auto-reconnect to another server

# Check logs
# Should see: "Reconnecting..."
# Should see: "Agent registered on server-X"
```

## Systemd Service

Create `/etc/systemd/system/jrok.service`:

```ini
[Unit]
Description=Jrok Tunnel Server
After=network.target

[Service]
Type=simple
User=jrok
Group=jrok
WorkingDirectory=/opt/jrok
EnvironmentFile=/opt/jrok/.env
ExecStart=/opt/jrok/jrok-server
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

Enable:

```bash
systemctl daemon-reload
systemctl enable jrok
systemctl start jrok
```

## Monitoring

### Health Check Endpoint

```bash
curl http://localhost:8080/health
# Returns: {"status":"ok"}
```

### Logs

```bash
# View logs
journalctl -u jrok -f

# Check for errors
journalctl -u jrok | grep ERROR
```

### Metrics (Optional - Add Later)

```bash
curl http://localhost:8080/metrics
# Could return Prometheus-format metrics
```

## Troubleshooting

### Server Won't Start

```bash
# Check database connection
turso db shell jrok "SELECT 1"

# Check environment variables
env | grep -E 'SERVER_ID|HTTP_HOST|TURSO'

# Check logs
RUST_LOG=debug ./jrok-server
```

### Agents Can't Connect

```bash
# Check if server is accessible
curl http://tunnel1.example.com:8080/health

# Check WebSocket endpoint
wscat -c ws://tunnel1.example.com:8080/ws/agent

# Check logs for registration errors
```

### Requests Timeout

```bash
# Check if agent is registered
turso db shell jrok "SELECT * FROM agents WHERE active = 1"

# Check agent heartbeat
turso db shell jrok "SELECT last_heartbeat FROM agents WHERE subdomain = 'myapp'"

# If heartbeat old, agent disconnected
```

### Load Balancer Shows Unhealthy

```bash
# Check /health endpoint
curl -v http://localhost:8080/health

# Check if port is open
netstat -tlnp | grep 8080

# Check firewall
ufw status
ufw allow 8080/tcp
```

## Scaling

### Add More Servers

1. Deploy new server with unique SERVER_ID
2. Add to Cloudflare LB pool
3. Server automatically joins cluster
4. Starts receiving traffic immediately

### Remove Server

1. Remove from Cloudflare LB pool
2. Stop server process
3. Agents auto-reconnect to other servers
4. Cluster continues operating

### Upgrade Cluster

```bash
# Rolling update (no downtime)
# 1. Update server 1
systemctl stop jrok
cp jrok-server-new /opt/jrok/jrok-server
systemctl start jrok

# 2. Wait for healthy
curl http://tunnel1.example.com:8080/health

# 3. Repeat for server 2, 3, etc
```
