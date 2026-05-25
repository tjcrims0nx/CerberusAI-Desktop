#!/bin/bash
sed -i '/# Internal endpoint for SmarterRouter/,$d' /etc/nginx/sites-enabled/ollama

cat << 'EOF' >> /etc/nginx/sites-enabled/ollama

# Internal endpoint for SmarterRouter to use the load balancer
server {
    listen 10.42.1.1:11434;
    
    location / {
        proxy_pass http://ollama_backend;
        proxy_http_version 1.1;
        proxy_set_header Host 127.0.0.1:11434;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # Streaming-friendly
        proxy_buffering off;
        proxy_request_buffering off;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
        proxy_set_header Connection "";
    }
}
EOF

nginx -t && systemctl reload nginx
