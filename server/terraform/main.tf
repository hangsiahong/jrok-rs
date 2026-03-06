variable "do_token" {
  description = "DigitalOcean API token"
  type        = string
  sensitive   = true
}

variable "turso_url" {
  description = "Turso database URL"
  type        = string
  sensitive   = true
}

variable "turso_token" {
  description = "Turso auth token"
  type        = string
  sensitive   = true
}

variable "base_domain" {
  description = "Base domain for tunnels (e.g., tunnel.example.com)"
  type        = string
}

variable "server_count" {
  description = "Number of servers to deploy"
  default     = 3
}

provider "digitalocean" {
  token = var.do_token
}

resource "digitalocean_droplet" "jrok" {
  count = var.server_count
  
  image  = "ubuntu-22-04-x64"
  name   = "jrok-server-${count.index + 1}"
  region = "nyc1"
  size   = "s-1vcpu-1gb"
  
  tags = ["jrok"]
}

output "server_ips" {
  description = "Deployed server IP addresses"
  value       = digitalocean_droplet.jrok[*].ipv4_address
}

output "next_steps" {
  value = <<-EOT
Deployed ${var.server_count} jrok servers!

Server IPs:
${join("\n", digitalocean_droplet.jrok[*].ipv4_address)}

Next Steps:
1. Build and copy jrok-server binary to each server
2. Setup Turso database
3. Configure environment variables
4. Start the service
5. Setup Cloudflare DNS/LB
EOT
}
