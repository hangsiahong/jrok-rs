#!/bin/bash
set -e

echo "🚀 Quick Deploy - Single Server"
echo ""

if [ -z "$TURSO_URL" ]; then
    echo "❌ TURSO_URL not set"
    echo "   Get it from: https://turso.tech"
    echo "   Example: export TURSO_URL=https://your-db.turso.io"
    exit 1
fi

if [ -z "$TURSO_TOKEN" ]; then
    echo "❌ TURSO_TOKEN not set"
    echo "   Get it from: turso db tokens create your-db"
    exit 1
fi

if [ -z "$DOMAIN" ]; then
    echo "❌ DOMAIN not set"
    echo "   Example: export DOMAIN=tunnel.example.com"
    exit 1
fi

SERVER_ID=${SERVER_ID:-$(hostname)}
HTTP_HOST=${HTTP_HOST:-$(curl -s ifconfig.me)}
HTTP_PORT=${HTTP_PORT:-8080}
TCP_HOST=${TCP_HOST:-$(curl -s ifconfig.me)}
BASE_DOMAIN=$DOMAIN

echo "📦 Building jrok..."
cargo build --release

echo "📝 Creating .env file..."
cat > .env << EOF
SERVER_ID=$SERVER_ID
HTTP_HOST=$HTTP_HOST
HTTP_PORT=$HTTP_PORT
TCP_HOST=$TCP_HOST
BASE_DOMAIN=$BASE_DOMAIN
TURSO_URL=$TURSO_URL
TURSO_TOKEN=$TURSO_TOKEN
EOF

echo ""
echo "✅ Build complete!"
echo ""
echo "Next steps:"
echo "1. Run: ./target/release/jrok-server"
echo "2. Setup Cloudflare:"
echo "   - Add DNS: *.tunnel.example.com -> $HTTP_HOST"
echo "   - Enable proxy (orange cloud) for free SSL"
echo ""
