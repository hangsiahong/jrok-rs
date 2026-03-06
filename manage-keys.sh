#!/bin/bash
# jrok API Key Management Script
# Usage: ./manage-keys.sh <create|list|delete> [args]

set -e

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Load environment variables
if [ -f .env ]; then
    export $(cat .env | grep -v '^#' | xargs)
else
    echo -e "${RED}Error: .env file not found${NC}"
    echo "Please create a .env file with TURSO_URL and TURSO_TOKEN"
    exit 1
fi

# Check required environment variables
if [ -z "$TURSO_URL" ] || [ -z "$TURSO_TOKEN" ]; then
    echo -e "${RED}Error: TURSO_URL and TURSO_TOKEN must be set in .env${NC}"
    exit 1
fi

# Function to create API key
create_key() {
    local name="$1"
    if [ -z "$name" ]; then
        echo -e "${RED}Error: Name is required${NC}"
        echo "Usage: $0 create <name>"
        exit 1
    fi

    local key_id=$(uuidgen)
    local key_value="jrok_$(uuidgen)"
    local key_prefix=$(echo "$key_value" | cut -c1-8)
    local now=$(date +%s)000

    # Generate HMAC-SHA256 hash
    local key_hash=$(echo -n "$key_value" | openssl dgst -sha256 -hmac "jrok-api-key-v1" -binary | base64)

    # Insert into database
    sqlite3 <<EOF
.mode json
ATTACH DATABASE 'file:$TURSO_URL?auth=$TURSO_TOKEN' AS turso;
INSERT INTO turso.api_keys (id, key_hash, key_prefix, name, created_at)
VALUES ('$key_id', '$key_hash', '$key_prefix', '$name', $now);
SELECT '✅ API Key Created Successfully!' as result;
EOF

    echo ""
    echo -e "${GREEN}✅ API Key Created Successfully!${NC}"
    echo "ID:        $key_id"
    echo "Name:      $name"
    echo "Key:       $key_value"
    echo "Prefix:    $key_prefix"
    echo ""
    echo -e "${YELLOW}⚠️  SAVE THIS KEY NOW - it won't be shown again!${NC}"
    echo ""
    echo "Use this key in your agent registration:"
    echo ""
    echo "{\"type\":\"register\",\"subdomain\":\"myapp\",\"local_port\":3000,\"local_host\":\"localhost\",\"protocol\":\"http\",\"api_key\":\"$key_value\"}"
}

# Function to list API keys
list_keys() {
    echo -e "${GREEN}📋 API Keys:${NC}"
    echo ""

    sqlite3 <<EOF
.mode column
.headers on
.width 15 30 15 25
SELECT
    id as 'ID',
    ifnull(name, 'N/A') as 'Name',
    key_prefix as 'Prefix',
    datetime(created_at/1000, 'unixepoch') as 'Created'
FROM api_keys
ORDER BY created_at DESC;
EOF
}

# Function to delete API key
delete_key() {
    local key_id="$1"
    if [ -z "$key_id" ]; then
        echo -e "${RED}Error: Key ID is required${NC}"
        echo "Usage: $0 delete <id>"
        exit 1
    fi

    sqlite3 <<EOF
DELETE FROM api_keys WHERE id = '$key_id';
SELECT changes() as deleted;
EOF

    echo -e "${GREEN}✅ API key deleted: $key_id${NC}"
}

# Function to show usage
show_usage() {
    echo "jrok API Key Management Tool"
    echo ""
    echo "Usage:"
    echo "  $0 create <name>    Create a new API key"
    echo "  $0 list             List all API keys"
    echo "  $0 delete <id>      Delete an API key"
    echo "  $0 help             Show this help"
    echo ""
    echo "Environment Variables (in .env):"
    echo "  TURSO_URL         Turso database URL"
    echo "  TURSO_TOKEN       Turso database auth token"
    echo ""
    echo "Examples:"
    echo "  $0 create \"Production Key\""
    echo "  $0 list"
    echo "  $0 delete <key-id>"
}

# Main command routing
case "${1:-help}" in
    create)
        create_key "$2"
        ;;
    list)
        list_keys
        ;;
    delete)
        delete_key "$2"
        ;;
    help|--help|-h)
        show_usage
        ;;
    *)
        echo -e "${RED}Unknown command: $1${NC}"
        echo ""
        show_usage
        exit 1
        ;;
esac
