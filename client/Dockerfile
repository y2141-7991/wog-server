# syntax=docker/dockerfile:1.7

# ---- Build -----------------------------------------------------------------
FROM node:22-alpine AS build

WORKDIR /app

# Dependencies first so they stay cached until package*.json changes.
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci

COPY . .

# Empty = the app calls /api and /gateway on its own origin, which nginx (below)
# proxies to the server. Set it only when the API lives on a different origin, e.g.
#   docker build --build-arg VITE_API_URL=https://api.example.com ./client
# (VITE_* values are baked into the JavaScript at build time.)
ARG VITE_API_URL=
ENV VITE_API_URL=$VITE_API_URL

RUN npm run build

# ---- Serve -----------------------------------------------------------------
FROM nginx:1.29-alpine AS runtime

# Files in /etc/nginx/templates are run through envsubst at start-up and written to
# /etc/nginx/conf.d, which is how BACKEND_URL gets in.
COPY nginx.conf.template /etc/nginx/templates/default.conf.template
COPY --from=build /app/dist /usr/share/nginx/html

# Where /api, /gateway and the support-chat WebSocket are proxied to.
ENV BACKEND_URL=http://server:3000
# nginx variable with the real client address (forwarded to the server for rate
# limiting). Behind a Cloudflare tunnel set it to $http_cf_connecting_ip.
ENV CLIENT_IP=\$remote_addr

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD wget -qO /dev/null http://127.0.0.1/ || exit 1
