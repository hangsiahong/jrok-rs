# API Key Management Guide

Since HTTP API endpoints are temporarily disabled due to dependency conflicts, here are **simple working methods** to manage API keys:

## Method 1: Using Turso CLI (Recommended)

### Install Turso CLI
```bash
curl -sSfL https://get.tur.so/install.sh | bash
```

### Create an API Key
```bash
# Generate a secure key
API_KEY="jrok_$(uuidgen | tr -d '-')"
KEY_PREFIX=$(echo "$API_KEY" | cut -c1-8)
KEY_ID=$(uuidgen | tr -d '-')
KEY_NAME="Production Key"

# Generate HMAC-SHA256 hash
KEY_HASH=$(echo -n "$API_KEY" | openssl dgst -sha256 -hmac "jrok-api-key-v1" -binary | base64)

# Insert into database
turso db shell $TURSO_URL <<SQL
INSERT INTO api_keys (id, key_hash, key_prefix, name, created_at)
VALUES ('$KEY_ID', '$KEY_HASH', '$KEY_PREFIX', '$KEY_NAME', $(date +%s)000);
SQL

echo "✅ API Key Created!"
echo "Key: $API_KEY"
```

### List API Keys
```bash
turso db shell $TURSO_URL "SELECT id, name, key_prefix, datetime(created_at/1000, 'unixepoch') as created FROM api_keys;"
```

### Delete an API Key
```bash
turso db shell $TURSO_URL "DELETE FROM api_keys WHERE id = 'YOUR_KEY_ID';"
```

## Method 2: Using Turso Shell

```bash
# Open interactive shell
turso db shell $TURSO_URL

# Then run SQL commands directly:
sqlite> INSERT INTO api_keys (id, key_hash, key_prefix, name, created_at)
   ... VALUES ('uuid', 'hash', 'prefix', 'My Key', $(date +%s)000);

sqlite> SELECT * FROM api_keys;
```

## Method 3: Direct HTTP API (Advanced)

You can also use Turso's HTTP API directly with curl:

```bash
# Execute SQL via HTTP API
curl -X POST "$TURSO_URL" \
  -H "Authorization: Bearer $TURSO_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "statements": [
      "SELECT id, name, key_prefix FROM api_keys"
    ]
  }'
```

## Using Your API Key

Once you have an API key, use it when registering your agent:

```json
{
  "type": "register",
  "subdomain": "myapp",
  "local_port": 3000,
  "local_host": "localhost",
  "protocol": "http",
  "api_key": "jrok_YOUR_KEY_HERE"
}
```

## Security Notes

1. **Never commit API keys to git**
2. **Store them securely** (use password manager or secrets manager)
3. **Rotate keys regularly** - delete old ones and create new ones
4. **Use descriptive names** to track what each key is for

## Quick Start Example

```bash
# 1. Set your Turso credentials
export TURSO_URL="libsql://your-db.turso.io"
export TURSO_TOKEN="your-auth-token"

# 2. Create a production key
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

# 3. Get your key ID and prefix
turso db shell $TURSO_URL "SELECT id, key_prefix FROM api_keys WHERE name = 'Production Agent';"
```

That's it! Your agent can now connect using this API key.
