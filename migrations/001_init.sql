CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    key_hash TEXT NOT NULL UNIQUE,
    key_prefix TEXT NOT NULL,
    name TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS servers (
    id TEXT PRIMARY KEY,
    http_host TEXT NOT NULL,
    tcp_host TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    last_heartbeat INTEGER NOT NULL,
    is_healthy INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS cluster_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    leader_id TEXT,
    leader_term INTEGER NOT NULL DEFAULT 0,
    last_heartbeat INTEGER NOT NULL
);

INSERT OR IGNORE INTO cluster_state (id, leader_id, leader_term, last_heartbeat)
VALUES (1, NULL, 0, 0);

CREATE TABLE IF NOT EXISTS tunnels (
    id TEXT PRIMARY KEY,
    subdomain TEXT UNIQUE NOT NULL,
    protocol TEXT NOT NULL DEFAULT 'http',
    tcp_port INTEGER,
    api_key_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (api_key_id) REFERENCES api_keys(id)
);

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    subdomain TEXT NOT NULL,
    server_id TEXT NOT NULL,
    tunnel_id TEXT,
    local_port INTEGER NOT NULL,
    local_host TEXT NOT NULL DEFAULT 'localhost',
    protocol TEXT NOT NULL DEFAULT 'http',
    connection_token TEXT NOT NULL,
    connected_at INTEGER NOT NULL,
    last_heartbeat INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (tunnel_id) REFERENCES tunnels(id),
    FOREIGN KEY (server_id) REFERENCES servers(id)
);

CREATE TABLE IF NOT EXISTS tcp_ports (
    port INTEGER PRIMARY KEY,
    tunnel_id TEXT NOT NULL,
    server_id TEXT NOT NULL,
    FOREIGN KEY (tunnel_id) REFERENCES tunnels(id),
    FOREIGN KEY (server_id) REFERENCES servers(id)
);

CREATE INDEX IF NOT EXISTS idx_agents_subdomain ON agents(subdomain, active);
CREATE INDEX IF NOT EXISTS idx_agents_server ON agents(server_id, active);
CREATE INDEX IF NOT EXISTS idx_tunnels_subdomain ON tunnels(subdomain);
CREATE INDEX IF NOT EXISTS idx_tcp_ports_tunnel ON tcp_ports(tunnel_id);
