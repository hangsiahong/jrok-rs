# Architecture

## Overview

Jrok-RS is a distributed tunnel service that uses Cloudflare for SSL and load balancing, eliminating the need for Nginx.

## Components

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│                     Cloudflare Network                      │
│                                                             │
│  ┌───────────────────────────────────────────────────┐    │
│  │       Cloudflare Load Balancer                    │    │
│  │  - SSL Termination (Free SSL)                    │    │
│  │  - Health Checks                                 │    │
│  │  - Round-robin Load Balancing                    │    │
│  └───────────────────────────────────────────────────┘    │
│                                                             │
└─────────────────────────────────────────────────────────────┘
                            │
        ┌──────────────────────────────────────────┐
        │                                          │
        │    Round-robin to ALL healthy servers    │
        │                                          │
        └──────────────────────────────────────────┘
                            │
        ┌───────────┬───────────┬───────────┐
        │           │           │           │
        ▼           ▼           ▼           ▼
   ┌─────────┐ ┌─────────┐ ┌─────────┐
   │Server A │ │Server B │ │Server C │
   │(Leader) │ │(Follower)│ │(Follower)│
   └─────────┘ └─────────┘ └─────────┘
        │           │           │
        └───────────┴───────────┘
                    │
                    ▼
            ┌──────────────┐
            │   Turso DB    │
            │ (Distributed) │
            └──────────────┘
```

## Load Balancing

### How it Works

**ALL servers receive traffic** (not just the leader):

```
Client Request
    │
    ▼
Cloudflare LB (round-robin)
    │
    ├────── Server A (leader)
    │
    ├────── Server B (follower)
    │
    └────── Server C (follower)
```

### What Leader Does

The leader only handles background tasks:
- Certificate requests (if needed in future)
- Cleanup of stale agents
- Monitoring cluster health

The leader does **NOT** route traffic. All servers route traffic.

### Example with 3 Servers

```
Request 1 → Server A (handles it)
Request 2 → Server B (handles it)
Request 3 → Server C (handles it)
Request 4 → Server A (handles it)
...
```

Each server:
1. Receives request from Cloudflare LB
2. Looks up agent in Turso DB
3. If agent is local → forward directly
4. If agent is remote → return redirect to correct server

## Single Server Setup

You can run with 1 server:

```
┌─────────────────────────────────────────┐
│                                        │
│  Client → Cloudflare LB → Server 1     │
│                         (still works)   │
│                                        │
└─────────────────────────────────────────┘
```

Benefits even with 1 server:
- Free SSL (Cloudflare handles it)
- Health checks
- DDoS protection
- Easy to scale (just add more servers)

## Why No Nginx?

### Traditional Setup (with Nginx)

```
Client → Cloudflare → Nginx → App Server → Agent
                         (extra layer)
```

### Jrok-RS Setup (No Nginx)

```
Client → Cloudflare → App Server → Agent
         (SSL here)   (HTTP only)
```

**Removed complexity:**
- No Nginx installation
- No Nginx config management
- No Nginx reload
- No port 80/443 management

**Cloudflare handles:**
- SSL termination
- Certificate renewal
- Load balancing
- Health checks

## Server Requirements

Each server needs:
1. Public IP address
2. HTTP port accessible (8080 or any port)
3. TCP port range accessible (10000-20000) for TCP tunnels

Cloudflare connects to your HTTP port.

## Deployment

### Minimal Setup (1 Server)

```bash
# Server 1
SERVER_ID=server-1 \
HTTP_HOST=tunnel1.example.com \
HTTP_PORT=8080 \
TCP_HOST=1.2.3.4 \
BASE_DOMAIN=tunnel.example.com \
TURSO_URL=https://xxx.turso.io \
TURSO_TOKEN=xxx \
./jrok-server
```

Cloudflare LB config:
- Origin: tunnel1.example.com:8080
- Health check: /health every 5s

### Production Setup (3+ Servers)

```bash
# Server 1
SERVER_ID=server-1 HTTP_HOST=server1.example.com:8080 ...

# Server 2
SERVER_ID=server-2 HTTP_HOST=server2.example.com:8080 ...

# Server 3
SERVER_ID=server-3 HTTP_HOST=server3.example.com:8080 ...
```

Cloudflare LB config:
- Origins: server1.example.com:8080, server2.example.com:8080, server3.example.com:8080
- Health check: /health every 5s
- Failover: 15s timeout

## How Request Routing Works

### Scenario 1: Agent on Same Server

```
Request arrives at Server A
    │
    ▼
Server A queries Turso: "Where is agent for subdomain 'myapp'?"
    │
    ▼
Turso: "Agent is on Server A"
    │
    ▼
Server A forwards to local agent
```

### Scenario 2: Agent on Different Server

```
Request arrives at Server A
    │
    ▼
Server A queries Turso: "Where is agent for subdomain 'myapp'?"
    │
    ▼
Turso: "Agent is on Server B"
    │
    ▼
Server A returns HTTP 307 redirect to Server B
    │
    ▼
Client reconnects to Server B
    │
    ▼
Server B handles request
```

Note: In practice, Cloudflare LB usually routes to the correct server because of session affinity or we could implement proxy forwarding.

## Cost Breakdown

### With Cloudflare LB ($6/month)

| Component | Cost |
|-----------|------|
| Cloudflare LB | $6/month |
| Turso | $0-10/month |
| 3x VPS ($5 each) | $15/month |
| **Total** | **$21-31/month** |

### Without LB (DNS Round-robin)

| Component | Cost |
|-----------|------|
| Cloudflare DNS | $0 |
| Turso | $0-10/month |
| 3x VPS ($5 each) | $15/month |
| **Total** | **$15-25/month** |

## FAQ

**Q: Do I need Cloudflare LB?**
A: No. You can use DNS round-robin. But LB provides health checks and instant failover.

**Q: Can I run with 1 server?**
A: Yes. Cloudflare LB still provides SSL and health checks.

**Q: Does leader handle all traffic?**
A: No. ALL servers handle traffic. Leader only does background tasks.

**Q: What if leader dies?**
A: Another server automatically becomes leader within 15 seconds.

**Q: Do I need Nginx?**
A: No. Cloudflare handles SSL. Your server runs HTTP only.

**Q: How do TCP tunnels work?**
A: TCP tunnels connect directly to server IP, not through Cloudflare.
