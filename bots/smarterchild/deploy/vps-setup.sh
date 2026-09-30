#!/usr/bin/env bash
# SmarterChild on a Docker host: installs it the first time, updates it after that.
# Run as root. It lives in /opt/docker/smarterchild beside the other services:
#   src/                 the code (just bots/smarterchild from github.com/tagban/him)
#   .env                 its settings and password (asked for once, mode 600)
#   data/                what it remembers (owned by uid 1000, the container's user)
#   docker-compose.yml
# Run it again any time to pull the latest code and restart.
set -euo pipefail

DIR="${SMARTERCHILD_DIR:-/opt/docker/smarterchild}"
REPO="https://github.com/tagban/him.git"

say() { printf '\n== %s\n' "$*"; }

if [ "$(id -u)" -ne 0 ] && [ -z "${SMARTERCHILD_DIR:-}" ]; then
  echo "Run this as root (it writes to $DIR and runs Docker)." >&2
  exit 1
fi
command -v docker >/dev/null || { echo "Docker isn't installed." >&2; exit 1; }
command -v git >/dev/null || { echo "git isn't installed." >&2; exit 1; }

mkdir -p "$DIR"
cd "$DIR"

say "Code"
if [ -d src/.git ]; then
  git -C src pull --ff-only
else
  git clone --depth 1 --filter=blob:none --sparse "$REPO" src
  git -C src sparse-checkout set bots/smarterchild
fi
git -C src log -1 --format='%h %s (%cr)'

say "Settings"
if [ -f .env ]; then
  echo "Keeping $DIR/.env (delete it and run this again to start over)."
else
  read -rp "Screen name on VesperNet [smarterchild]: " LOGIN
  LOGIN="${LOGIN:-smarterchild}"
  while true; do
    read -rsp "Password for $LOGIN: " PASSWORD; echo
    [ -n "$PASSWORD" ] || { echo "It needs a password."; continue; }
    case "$PASSWORD" in *"'"*) echo "Sorry, a password with ' in it can't go in .env. Change it and try again."; exit 1 ;; esac
    break
  done
  read -rp "Also sit in the Hotline Central Hub's public chat? [Y/n]: " HUB
  HUB_HOST=""
  if [[ ! "$HUB" =~ ^[Nn] ]]; then
    read -rp "Hub address [74.208.191.206]: " HUB_HOST
    HUB_HOST="${HUB_HOST:-74.208.191.206}"
  fi
  umask 077
  cat > .env <<EOF
# SmarterChild's settings. Values in single quotes are taken literally.
HOTLINE_HOST=hotline.vespernet.net
HOTLINE_PORT=5500
SMARTERCHILD_LOGIN=$LOGIN
SMARTERCHILD_PASSWORD='$PASSWORD'
SMARTERCHILD_NAME=SmarterChild
SMARTERCHILD_STATUS='Ask me anything! Type "help".'

# Public chat (empty HUB_HOST = stay out). Guests by default; fill in an account if needed.
HUB_HOST=$HUB_HOST
HUB_PORT=5500
HUB_LOGIN=
HUB_PASSWORD=
HUB_ICON=168
EOF
  umask 022
  unset PASSWORD
  echo "Saved $DIR/.env (readable by root only)."
fi
chmod 600 .env

mkdir -p data
chown 1000:1000 data

cat > docker-compose.yml <<'EOF'
# Written by src/bots/smarterchild/deploy/vps-setup.sh; run that again to update.
services:
  smarterchild:
    build: ./src/bots/smarterchild
    image: smarterchild:local
    container_name: smarterchild
    restart: unless-stopped
    env_file: .env
    volumes:
      - ./data:/data
    # A Hotline server on this same machine (HUB_HOST=host.docker.internal)
    extra_hosts:
      - "host.docker.internal:host-gateway"
    logging:
      driver: json-file
      options:
        max-size: "10m"
        max-file: "3"
EOF

say "Starting"
docker compose up -d --build
sleep 8
docker compose ps
say "Log (docker compose -f $DIR/docker-compose.yml logs -f to follow)"
docker compose logs --tail 15
