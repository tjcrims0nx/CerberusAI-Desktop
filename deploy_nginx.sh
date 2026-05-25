cat << 'EOF' > /etc/nginx/sites-enabled/api.cerberusai.dev
server {
    listen 80;
    server_name api.cerberusai.dev;
    return 301 https://$host$request_uri;
}

server {
    listen 443 ssl;
    server_name api.cerberusai.dev;

    ssl_certificate /etc/letsencrypt/live/api.cerberusai.dev/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.cerberusai.dev/privkey.pem;
    include /etc/letsencrypt/options-ssl-nginx.conf;
    ssl_dhparam /etc/letsencrypt/ssl-dhparams.pem;

    add_header X-Content-Type-Options nosniff always;
    add_header X-Frame-Options DENY always;
    add_header X-XSS-Protection "1; mode=block" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;
    add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;

    # Rate-limit API gateway
    limit_req zone=api_limit burst=30 nodelay;

    location /skills-sse {
        # CORS Headers
        add_header 'Access-Control-Allow-Origin' '*' always;
        add_header 'Access-Control-Allow-Methods' 'GET, POST, OPTIONS' always;
        add_header 'Access-Control-Allow-Headers' 'DNT,User-Agent,X-Requested-With,If-Modified-Since,Cache-Control,Content-Type,Range,Authorization' always;
        add_header 'Access-Control-Expose-Headers' 'Content-Length,Content-Range' always;

        if ($request_method = 'OPTIONS') {
            add_header 'Access-Control-Allow-Origin' '*' always;
            add_header 'Access-Control-Allow-Methods' 'GET, POST, OPTIONS' always;
            add_header 'Access-Control-Allow-Headers' 'DNT,User-Agent,X-Requested-With,If-Modified-Since,Cache-Control,Content-Type,Range,Authorization' always;
            add_header 'Access-Control-Max-Age' 1728000;
            add_header 'Content-Type' 'text/plain; charset=utf-8';
            add_header 'Content-Length' 0;
            return 204;
        }

        proxy_pass http://localhost:3001/skills-sse;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header Connection '';
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 86400s;
        proxy_send_timeout 86400s;
    }

    location /skills-message {
        # CORS Headers
        add_header 'Access-Control-Allow-Origin' '*' always;
        add_header 'Access-Control-Allow-Methods' 'GET, POST, OPTIONS' always;
        add_header 'Access-Control-Allow-Headers' 'DNT,User-Agent,X-Requested-With,If-Modified-Since,Cache-Control,Content-Type,Range,Authorization' always;
        add_header 'Access-Control-Expose-Headers' 'Content-Length,Content-Range' always;

        if ($request_method = 'OPTIONS') {
            add_header 'Access-Control-Allow-Origin' '*' always;
            add_header 'Access-Control-Allow-Methods' 'GET, POST, OPTIONS' always;
            add_header 'Access-Control-Allow-Headers' 'DNT,User-Agent,X-Requested-With,If-Modified-Since,Cache-Control,Content-Type,Range,Authorization' always;
            add_header 'Access-Control-Max-Age' 1728000;
            add_header 'Content-Type' 'text/plain; charset=utf-8';
            add_header 'Content-Length' 0;
            return 204;
        }

        proxy_pass http://localhost:3001/skills-message;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
    }

    location /__k8s-api/ {
        limit_req zone=api_limit burst=30 nodelay;
        rewrite ^/__k8s-api/?(.*)$ /$1 break;
        proxy_pass http://10.43.5.61:80;
        proxy_http_version 1.1;
        proxy_set_header Host api-bridge.internal;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header Connection "";
        proxy_buffering off;
        proxy_request_buffering off;
        proxy_read_timeout 600s;
        proxy_connect_timeout 600s;
    }

    location / {
        proxy_pass http://10.43.5.61:80;
        proxy_http_version 1.1;
        proxy_set_header Host api-bridge.internal;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_cache_bypass $http_upgrade;

        # Generous timeout for LLM queries
        proxy_read_timeout 600s;
        proxy_connect_timeout 600s;

        # Disable buffering for real-time SSE token streaming
        proxy_buffering off;
        proxy_request_buffering off;
    }
}
EOF
nginx -t && systemctl reload nginx
