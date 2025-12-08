#!/bin/bash
# =============================================================================
# Certbot SSL Setup Script
# =============================================================================
# Automates Let's Encrypt SSL certificate setup with nginx
# Usage: sudo ./certbot-setup.sh yourdomain.com [email@example.com]
# =============================================================================
set -euo pipefail

DOMAIN="${1:-}"
EMAIL="${2:-}"

if [[ -z "$DOMAIN" ]]; then
    echo "Usage: $0 <domain> [email]"
    echo "Example: $0 api.viralclipai.com admin@viralclipai.com"
    exit 1
fi

# Default email
EMAIL="${EMAIL:-admin@${DOMAIN}}"

echo "======================================"
echo "Setting up SSL for: $DOMAIN"
echo "Certificate notifications: $EMAIL"
echo "======================================"

# Create webroot directory for Let's Encrypt challenges
mkdir -p /var/www/certbot

# Create temporary nginx config for certificate validation
cat > /etc/nginx/sites-available/certbot-temp << EOF
server {
    listen 80;
    listen [::]:80;
    server_name $DOMAIN;
    
    location /.well-known/acme-challenge/ {
        root /var/www/certbot;
    }
    
    location / {
        return 301 https://\$host\$request_uri;
    }
}
EOF

# Enable temp config
ln -sf /etc/nginx/sites-available/certbot-temp /etc/nginx/sites-enabled/
rm -f /etc/nginx/sites-enabled/default 2>/dev/null || true

# Test and reload nginx
nginx -t && systemctl reload nginx

echo "Obtaining SSL certificate..."

# Get certificate
certbot certonly \
    --webroot \
    --webroot-path=/var/www/certbot \
    --email "$EMAIL" \
    --agree-tos \
    --no-eff-email \
    --non-interactive \
    -d "$DOMAIN"

# Verify certificate exists
if [[ -f "/etc/letsencrypt/live/$DOMAIN/fullchain.pem" ]]; then
    echo "SSL certificate obtained successfully!"
else
    echo "ERROR: Certificate was not created"
    exit 1
fi

# Update nginx config with actual domain
if [[ -f /etc/nginx/nginx.conf.template ]]; then
    sed "s/api.yourdomain.com/$DOMAIN/g" /etc/nginx/nginx.conf.template > /etc/nginx/nginx.conf
fi

# Remove temp config
rm -f /etc/nginx/sites-enabled/certbot-temp
rm -f /etc/nginx/sites-available/certbot-temp

# Setup auto-renewal cron job
if ! crontab -l 2>/dev/null | grep -q certbot; then
    (crontab -l 2>/dev/null; echo "0 3 * * * /usr/bin/certbot renew --quiet --post-hook 'systemctl reload nginx'") | crontab -
    echo "Auto-renewal cron job added"
fi

# Test nginx config and reload
nginx -t && systemctl reload nginx

echo "======================================"
echo "SSL Setup Complete!"
echo "======================================"
echo "Certificate location: /etc/letsencrypt/live/$DOMAIN/"
echo "Auto-renewal: Enabled (daily at 3 AM)"
echo ""
echo "Test your SSL at: https://www.ssllabs.com/ssltest/analyze.html?d=$DOMAIN"
echo "======================================"
