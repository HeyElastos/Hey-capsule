#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
#  Hyper mesh hub — one-command self-hosted relay installer / updater.
#
#  Stands up (or UPDATES) an `iroh-relay` PINNED to the EXACT iroh version the
#  Hyper app uses (1.0.2 stable — a version mismatch silently breaks the mesh),
#  dual-stack (IPv4+IPv6), automatic Let's Encrypt TLS, hardened systemd service.
#
#  ── SMART MODE (auto) ──
#   • If NO relay is installed yet  → full INSTALL (packages, hardening, firewall,
#     TLS config, systemd service).
#   • If a relay is ALREADY installed → fast UPDATE: rebuild the binary to the
#     pinned version + restart, and leave your hardening / firewall / config
#     untouched. Exactly what you want after bumping the app's iroh version.
#   Force a full re-install with  FORCE_INSTALL=1.
#
#  ── NO DOMAIN NEEDED ──
#   Pass EITHER a real domain OR just your server's public IP. With an IP, the
#   script uses a FREE sslip.io hostname (<ip>.sslip.io) so Let's Encrypt can
#   still issue a real TLS cert — no domain purchase.
#
#  ── PREREQUISITES ──
#   - Real domain: point its A (IPv4) + AAAA (IPv6) records here first.
#   - IP only: nothing to set up — sslip.io resolves it automatically.
#   - Run as root (sudo). Ports 80/tcp, 443/tcp, 7842/udp must be reachable.
#
#  ── USAGE ──
#      sudo bash deploy-hey-relay.sh                              # auto: install OR update
#      sudo bash deploy-hey-relay.sh                              #   (re-run any time to update to the pinned version)
#      sudo DOMAIN=203.0.113.7        bash deploy-hey-relay.sh    # new install at a specific IP
#      sudo DOMAIN=relay.example.com  bash deploy-hey-relay.sh    # new install with your own domain
#      sudo DOMAIN=relay.example.com  bash deploy-hey-relay.sh    # (in update mode, also re-points an existing relay)
#      sudo FORCE_INSTALL=1           bash deploy-hey-relay.sh    # force the full install path
#
#  Then paste the printed https://… URL into Hyper on BOTH phones
#  (Profile → Connection → "My own relay"), and fully reopen the app.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

DOMAIN="${DOMAIN:-}"
PREFERRED_DOMAIN="${PREFERRED_DOMAIN:-}"   # empty = default to auto sslip.io; set to elastos.app / your domain to use a real one
EMAIL="${EMAIL:-}"                          # ACME/Let's Encrypt account email (required by iroh-relay for LetsEncrypt)
RELAY_VERSION="1.0.2"                       # MUST match the iroh version in the Hyper app.
QUIC_PORT=7842                              # iroh-relay DEFAULT_RELAY_QUIC_PORT (UDP)
FORCE_INSTALL="${FORCE_INSTALL:-0}"         # set 1 to force the full install even if a relay is already present

BIN=/usr/local/bin/iroh-relay
CONFIG=/etc/iroh-relay/config.toml
SERVICE=iroh-relay.service

[ "$(id -u)" -eq 0 ] || { echo "ERROR: run as root (sudo)."; exit 1; }

# ── helpers ──────────────────────────────────────────────────────────────────
ensure_rust() {
  if ! command -v cargo >/dev/null 2>&1; then
    echo "==> installing Rust toolchain"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
  fi
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
  rustup default stable >/dev/null 2>&1 || true   # iroh 1.0.2 needs a recent rustc (>=1.91; 2026 stable is fine)
}

# Build + install iroh-relay at the pinned version onto a stable path.
build_relay() {
  echo "==> building + installing iroh-relay $RELAY_VERSION (exact match to the Hyper app)"
  cargo install iroh-relay --version "$RELAY_VERSION" --features server --locked \
    || cargo install iroh-relay --version "$RELAY_VERSION" --features server
  install -m 0755 "$(command -v iroh-relay)" "$BIN"
  echo "    installed: $("$BIN" --version 2>/dev/null || echo "$BIN")"
}

# The version string the installed binary reports (e.g. "iroh-relay 1.0.2" → 1.0.2).
installed_version() { "$BIN" --version 2>/dev/null | awk '{print $NF}'; }

# Print health after (re)start so you can SEE it came up + the cert issued.
verify_relay() {
  echo
  echo "==> verify"
  local active; active="$(systemctl is-active "$SERVICE" 2>/dev/null || true)"
  echo "    service : $active   (version $(installed_version))"
  echo "    recent log:"
  journalctl -u "$SERVICE" --since "90 sec ago" --no-pager 2>/dev/null \
    | grep -iE 'listen|cert|acme|relay|error|warn' | tail -6 | sed 's/^/      /' || true
  # A LetsEncrypt cert takes ~30–60s on first boot; the HTTPS probe may 4xx until then — that's fine.
  local code; code="$(curl -4 -fsS -o /dev/null -w '%{http_code}' --max-time 8 "https://$HOST/" 2>/dev/null || echo "…")"
  echo "    https probe: HTTP $code  (any response = TLS is up; 404 is normal for '/')"
}

# ── detect an existing install → fast UPDATE path ────────────────────────────
is_installed() {
  [ -x "$BIN" ] && [ -f "$CONFIG" ] && systemctl cat "$SERVICE" >/dev/null 2>&1
}

if is_installed && [ "$FORCE_INSTALL" != "1" ]; then
  CUR="$(installed_version)"
  CUR_HOST="$(awk -F'"' '/^hostname[[:space:]]*=/{print $2; exit}' "$CONFIG" 2>/dev/null)"
  echo "================================================================"
  echo "  Existing Hyper relay detected — UPDATE mode."
  echo "    host    : ${CUR_HOST:-unknown}"
  echo "    version : ${CUR:-unknown}   →   target ${RELAY_VERSION}"
  echo "    (run with FORCE_INSTALL=1 to redo the full install instead.)"
  echo "================================================================"
  ensure_rust

  # Optional re-point: a DOMAIN= that differs from the current cert hostname
  # rewrites the config so you can move an existing relay to a new name/IP.
  if [ -n "$DOMAIN" ]; then
    NEWHOST="$DOMAIN"
    [[ "$DOMAIN" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]] && NEWHOST="${DOMAIN}.sslip.io"
    if [ -n "$CUR_HOST" ] && [ "$NEWHOST" != "$CUR_HOST" ]; then
      echo "==> re-pointing relay: $CUR_HOST → $NEWHOST"
      sed -i -E "s|^hostname[[:space:]]*=.*|hostname = \"$NEWHOST\"|" "$CONFIG"
      HOST="$NEWHOST"
    else
      HOST="$CUR_HOST"
    fi
  else
    HOST="$CUR_HOST"
  fi

  # Only rebuild if the binary version differs (cargo install is idempotent, but
  # this makes "already up to date" instant and honest).
  if [ "$CUR" != "$RELAY_VERSION" ]; then
    build_relay
  else
    echo "==> binary already at $RELAY_VERSION — skipping rebuild (restarting to apply any config change)."
  fi

  systemctl daemon-reload 2>/dev/null || true
  systemctl restart "$SERVICE"
  verify_relay
  echo
  echo "================================================================"
  echo "  Update complete. Relay URL (paste into Hyper → My own relay):"
  echo
  echo "        https://${HOST}"
  echo
  echo "    status: systemctl status iroh-relay   |   logs: journalctl -u iroh-relay -f"
  echo "================================================================"
  exit 0
fi

# ─────────────────────────────────────────────────────────────────────────────
#  FULL INSTALL  (no relay present, or FORCE_INSTALL=1)
# ─────────────────────────────────────────────────────────────────────────────
echo "==> 1/7  base packages + Rust toolchain"
export DEBIAN_FRONTEND=noninteractive
apt-get update -y
apt-get install -y curl build-essential pkg-config libssl-dev ca-certificates ufw fail2ban

# ── Resolve the relay hostname ──
# Priority: an explicit DOMAIN= you pass, ELSE try $PREFERRED_DOMAIN but ONLY if
# its DNS actually points at THIS server (else Let's Encrypt validates the wrong
# box and the cert never issues), ELSE a free sslip.io hostname.
if [ -z "$DOMAIN" ]; then
  echo "==> detecting this server's public IPv4…"
  MYIP=""
  for svc in "https://api.ipify.org" "https://ipv4.icanhazip.com" "https://ifconfig.me/ip"; do
    MYIP="$(curl -4 -fsS --max-time 10 "$svc" 2>/dev/null | tr -d '[:space:]')"
    [[ "$MYIP" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]] && break
    MYIP=""
  done
  [ -n "$MYIP" ] || { echo "ERROR: couldn't detect this server's public IPv4. Set DOMAIN=<ip-or-domain>."; exit 1; }
  echo "    this server: $MYIP"
  PDIP="$(getent ahostsv4 "$PREFERRED_DOMAIN" 2>/dev/null | awk '{print $1; exit}')"
  if [ -n "$PREFERRED_DOMAIN" ] && [ "$PDIP" = "$MYIP" ]; then
    DOMAIN="$PREFERRED_DOMAIN"
    echo "==> $PREFERRED_DOMAIN points here -> using it."
  else
    DOMAIN="$MYIP"
    [ -n "$PREFERRED_DOMAIN" ] && echo "==> $PREFERRED_DOMAIN does NOT point here (resolves to ${PDIP:-nothing}; this box is $MYIP). Falling back to sslip.io."
  fi
fi
# A bare IPv4 → free sslip.io hostname (Let's Encrypt can cert it; no domain to buy).
if [[ "$DOMAIN" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
  HOST="${DOMAIN}.sslip.io"
  echo "==> Using free auto-hostname  $HOST  (sslip.io -> $DOMAIN)"
else
  HOST="$DOMAIN"
fi
# Let's Encrypt requires a contact email (NOT verified — just where expiry notices go).
EMAIL="${EMAIL:-admin@$HOST}"
echo "==> ACME contact email: $EMAIL  (override with EMAIL=you@example.com)"

ensure_rust

echo "==> 2/7  build + install iroh-relay $RELAY_VERSION"
build_relay

echo "==> 3/7  kernel hardening (SYN-flood / spoof / conn-exhaustion resistance)"
cat > /etc/sysctl.d/99-hey-relay.conf <<'EOF'
# One [::] socket serves both IPv4 + IPv6 (dual-stack).
net.ipv6.bindv6only = 0
# SYN-flood mitigation.
net.ipv4.tcp_syncookies = 1
net.ipv4.tcp_max_syn_backlog = 4096
net.core.somaxconn = 8192
net.ipv4.tcp_synack_retries = 3
# TIME-WAIT assassination protection.
net.ipv4.tcp_rfc1337 = 1
# Anti-spoofing (reverse-path filter) + log spoofed packets.
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1
net.ipv4.conf.all.log_martians = 1
# Ignore ICMP redirects + source routing (no MITM re-routing of our traffic).
net.ipv4.conf.all.accept_redirects = 0
net.ipv4.conf.default.accept_redirects = 0
net.ipv4.conf.all.send_redirects = 0
net.ipv4.conf.all.accept_source_route = 0
net.ipv6.conf.all.accept_redirects = 0
net.ipv6.conf.all.accept_source_route = 0
# Drop smurf-attack broadcast pings.
net.ipv4.icmp_echo_ignore_broadcasts = 1
net.ipv4.icmp_ignore_bogus_error_responses = 1
# Bigger connection-tracking table (many concurrent peers).
net.netfilter.nf_conntrack_max = 262144
EOF
sysctl --system >/dev/null 2>&1 || true

echo "==> 4/7  relay config ($CONFIG)"
mkdir -p /etc/iroh-relay
cat > "$CONFIG" <<EOF
# Hyper mesh hub — dual-stack, automatic Let's Encrypt TLS. iroh-relay $RELAY_VERSION.
enable_relay = true
# Bind [::] so ONE socket serves BOTH IPv4 and IPv6 (bindv6only=0 above).
http_bind_addr = "[::]:80"            # HTTP: ACME challenge + captive-portal probe
enable_quic_addr_discovery = true     # QAD: lets phones learn their public address -> more DIRECT links
enable_metrics = true
metrics_bind_addr = "127.0.0.1:9090"  # localhost ONLY — never expose metrics to the internet

[tls]
hostname = "$HOST"
cert_mode = "LetsEncrypt"             # free auto-renewing certificate; no certbot needed
contact = "$EMAIL"                    # REQUIRED for LetsEncrypt; the CLI adds the mailto: prefix
prod_tls = true                       # production Let's Encrypt (set false to test against staging first)
https_bind_addr = "[::]:443"          # the endpoint phones connect to
quic_bind_addr = "[::]:$QUIC_PORT"    # QUIC relay + address discovery (UDP $QUIC_PORT)
cert_dir = "/var/lib/iroh-relay/certs"
EOF
chmod 0644 "$CONFIG"

echo "==> 5/7  hardened systemd service"
cat > /etc/systemd/system/iroh-relay.service <<EOF
[Unit]
Description=Hyper mesh hub (iroh-relay $RELAY_VERSION)
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=$BIN --config-path $CONFIG
Restart=always
RestartSec=3

# Run unprivileged; only allowed to bind the low ports.
DynamicUser=yes
StateDirectory=iroh-relay
AmbientCapabilities=CAP_NET_BIND_SERVICE
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

# Sandbox: contain the process if it's ever compromised.
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictNamespaces=yes
LockPersonality=yes
MemoryDenyWriteExecute=yes
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX AF_NETLINK
SystemCallArchitectures=native

# Resource caps: a flood can't OOM the box or exhaust fds.
LimitNOFILE=1048576
TasksMax=8192
MemoryMax=3G

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now iroh-relay

echo "==> 6/7  hardened firewall (default-DENY in, rate-limited SSH, only relay ports)"
sed -i 's/^IPV6=.*/IPV6=yes/' /etc/default/ufw 2>/dev/null || true
ufw default deny incoming  >/dev/null 2>&1 || true
ufw default allow outgoing >/dev/null 2>&1 || true
ufw default deny routed    >/dev/null 2>&1 || true
ufw limit OpenSSH >/dev/null 2>&1 || ufw limit 22/tcp >/dev/null 2>&1 || true
ufw allow 80/tcp           >/dev/null 2>&1 || true   # ACME HTTP-01 challenge
ufw allow 443/tcp          >/dev/null 2>&1 || true   # relay HTTPS — the brokered traffic
ufw allow "$QUIC_PORT"/udp >/dev/null 2>&1 || true   # QUIC relay + address discovery
ufw logging low            >/dev/null 2>&1 || true
ufw --force enable         >/dev/null 2>&1 || true
systemctl enable --now fail2ban >/dev/null 2>&1 || true

# ── SSH: key-only login (passwords disabled) — WITH a lockout safety gate ──
echo "==> 6b/7  SSH hardening (key-only login)"
KEYS_FOUND=0
for f in /root/.ssh/authorized_keys /home/*/.ssh/authorized_keys; do
  [ -s "$f" ] && grep -qE '^(ssh-|ecdsa-|sk-)' "$f" 2>/dev/null && { KEYS_FOUND=1; break; }
done
if [ "$KEYS_FOUND" -eq 1 ]; then
  for cfg in /etc/ssh/sshd_config /etc/ssh/sshd_config.d/*.conf; do
    [ -f "$cfg" ] || continue
    sed -i -E 's/^#?[[:space:]]*PasswordAuthentication[[:space:]]+.*/PasswordAuthentication no/I' "$cfg"
    sed -i -E 's/^#?[[:space:]]*KbdInteractiveAuthentication[[:space:]]+.*/KbdInteractiveAuthentication no/I' "$cfg"
    sed -i -E 's/^#?[[:space:]]*ChallengeResponseAuthentication[[:space:]]+.*/ChallengeResponseAuthentication no/I' "$cfg"
  done
  cat > /etc/ssh/sshd_config.d/99-hey-hardening.conf <<'SSHEOF'
# Hyper relay — key-only SSH. Public-key auth ONLY; passwords fully disabled.
PubkeyAuthentication yes
PasswordAuthentication no
KbdInteractiveAuthentication no
ChallengeResponseAuthentication no
PermitEmptyPasswords no
PermitRootLogin prohibit-password
SSHEOF
  if sshd -t 2>/dev/null; then
    systemctl reload ssh 2>/dev/null || systemctl reload sshd 2>/dev/null || true
    echo "    SSH is now KEY-ONLY (password login disabled)."
  else
    echo "    ! sshd config test FAILED — reload skipped (your session is safe). Run: sshd -t"
  fi
else
  echo "    SKIPPED: no SSH public key found in authorized_keys."
  echo "    Disabling passwords now would LOCK YOU OUT, so this step was skipped."
  echo "    Add your key first:  ssh-copy-id root@THIS_SERVER_IP  then re-run."
fi

echo "==> 7/7  verify + done"
verify_relay
echo
echo "================================================================"
echo "  Hyper mesh hub is running (iroh-relay $RELAY_VERSION)."
echo "    status:   systemctl status iroh-relay"
echo "    logs:     journalctl -u iroh-relay -f"
echo
echo "  Paste THIS into Hyper on BOTH phones"
echo "  (Profile → Connection → My own relay), then fully reopen Hyper:"
echo
echo "        https://$HOST"
echo
echo "  (First Let's Encrypt cert takes ~30-60s — watch the logs.)"
echo "  Re-run this script any time to UPDATE the relay to the pinned version."
echo "================================================================"
