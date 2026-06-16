#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
#  Hey mesh hub — one-command self-hosted relay installer.
#
#  Stands up an `iroh-relay` PINNED to 1.0.0-rc.1 (the EXACT version the Hey app
#  uses — a mismatch silently breaks the mesh), dual-stack (IPv4+IPv6), with
#  automatic Let's Encrypt TLS, run as a hardened systemd service.
#
#  ── NO DOMAIN NEEDED ──
#   You can pass EITHER a real domain OR just your server's public IP.
#   With an IP, the script auto-uses a FREE sslip.io hostname (<ip>.sslip.io)
#   so Let's Encrypt can still issue a real TLS cert — no domain purchase.
#
#  ── PREREQUISITES ──
#   - If using a real domain: point its A (IPv4) + AAAA (IPv6) records here first.
#   - If using an IP: nothing to set up — sslip.io resolves it automatically.
#   - Run as root (sudo). Ports 80/tcp, 443/tcp, 7842/udp must be reachable.
#
#  ── USAGE ──
#      sudo bash deploy-hey-relay.sh                              # ZERO-CONFIG: auto-detect IP + sslip.io
#      sudo DOMAIN=203.0.113.7        bash deploy-hey-relay.sh     # force a specific IP
#      sudo DOMAIN=relay.example.com bash deploy-hey-relay.sh     # use your own domain
#
#  Then paste the printed https://… URL into Hey on BOTH phones
#  (Profile -> Connection -> Relay server), and fully reopen the app.
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

DOMAIN="${DOMAIN:-}"
PREFERRED_DOMAIN="${PREFERRED_DOMAIN:-}"    # empty = default to auto sslip.io; set PREFERRED_DOMAIN=elastos.app or DOMAIN=elastos.app to use a real domain
EMAIL="${EMAIL:-}"                    # ACME/Let's Encrypt account email (required by iroh-relay rc.1)
RELAY_VERSION="1.0.0-rc.1"           # MUST match the iroh in the Hey app.
QUIC_PORT=7842                       # iroh-relay DEFAULT_RELAY_QUIC_PORT (UDP)

[ "$(id -u)" -eq 0 ] || { echo "ERROR: run as root (sudo)."; exit 1; }

echo "==> 1/7  base packages + Rust toolchain"
export DEBIAN_FRONTEND=noninteractive
apt-get update -y
apt-get install -y curl build-essential pkg-config libssl-dev ca-certificates ufw fail2ban

# ── Resolve the relay hostname ──
# Priority: an explicit DOMAIN= you pass, ELSE try $PREFERRED_DOMAIN (elastos.app)
# but ONLY if its DNS actually points at THIS server (else Let's Encrypt validates
# against the wrong box and the cert never issues), ELSE a free sslip.io hostname.
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
  # Does the preferred domain resolve to THIS server? (A-record check)
  PDIP="$(getent ahostsv4 "$PREFERRED_DOMAIN" 2>/dev/null | awk '{print $1; exit}')"
  if [ -n "$PREFERRED_DOMAIN" ] && [ "$PDIP" = "$MYIP" ]; then
    DOMAIN="$PREFERRED_DOMAIN"
    echo "==> $PREFERRED_DOMAIN points here -> using it."
  else
    DOMAIN="$MYIP"
    echo "==> $PREFERRED_DOMAIN does NOT point here (it resolves to ${PDIP:-nothing}; this box is $MYIP)."
    echo "    Falling back to a free sslip.io hostname so the cert still issues."
  fi
fi
# A bare IPv4 → free sslip.io hostname (Let's Encrypt can cert it; no domain to buy).
if [[ "$DOMAIN" =~ ^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ ]]; then
  HOST="${DOMAIN}.sslip.io"
  echo "==> Using free auto-hostname  $HOST  (sslip.io -> $DOMAIN)"
else
  HOST="$DOMAIN"
fi
# Let's Encrypt requires a contact email (it is NOT verified/reachable-checked —
# it's just where expiry notices go). Default to admin@<host>; override with EMAIL=.
EMAIL="${EMAIL:-admin@$HOST}"
echo "==> ACME contact email: $EMAIL  (override with EMAIL=you@example.com)"
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup default stable >/dev/null     # iroh 1.0-rc.1 needs a recent rustc (>=1.91; 2026 stable is fine)

echo "==> 2/7  build + install iroh-relay $RELAY_VERSION (exact match to the Hey app)"
cargo install iroh-relay --version "$RELAY_VERSION" --features server --locked \
  || cargo install iroh-relay --version "$RELAY_VERSION" --features server
# Put the binary on a stable, dynamic-user-readable path.
install -m 0755 "$(command -v iroh-relay)" /usr/local/bin/iroh-relay
echo "    installed: $(/usr/local/bin/iroh-relay --version 2>/dev/null || echo /usr/local/bin/iroh-relay)"

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

echo "==> 4/7  relay config (/etc/iroh-relay/config.toml)"
mkdir -p /etc/iroh-relay
cat > /etc/iroh-relay/config.toml <<EOF
# Hey mesh hub — dual-stack, automatic Let's Encrypt TLS.
enable_relay = true
# Bind [::] so ONE socket serves BOTH IPv4 and IPv6 (bindv6only=0 above).
http_bind_addr = "[::]:80"            # HTTP: ACME challenge + captive-portal probe
enable_quic_addr_discovery = true     # QAD: lets phones learn their public address -> more DIRECT links
enable_metrics = true
metrics_bind_addr = "127.0.0.1:9090"  # localhost ONLY — never expose metrics to the internet

[tls]
hostname = "$HOST"
cert_mode = "LetsEncrypt"             # free auto-renewing certificate; no certbot needed
contact = "$EMAIL"                    # REQUIRED by iroh-relay rc.1; the CLI adds the mailto: prefix
prod_tls = true                       # production Let's Encrypt (set false to test against staging first)
https_bind_addr = "[::]:443"          # the endpoint phones connect to
quic_bind_addr = "[::]:$QUIC_PORT"    # QUIC relay + address discovery (UDP $QUIC_PORT)
cert_dir = "/var/lib/iroh-relay/certs"
EOF
chmod 0644 /etc/iroh-relay/config.toml

echo "==> 5/7  hardened systemd service"
cat > /etc/systemd/system/iroh-relay.service <<EOF
[Unit]
Description=Hey mesh hub (iroh-relay $RELAY_VERSION)
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/iroh-relay --config-path /etc/iroh-relay/config.toml
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
# Manage IPv6 too (the relay is dual-stack).
sed -i 's/^IPV6=.*/IPV6=yes/' /etc/default/ufw 2>/dev/null || true
# Default-deny everything inbound; allow outbound (ACME + relay need it); the box
# is NOT a router, so drop any forwarded traffic. (Policies stage now, activate on enable.)
ufw default deny incoming  >/dev/null 2>&1 || true
ufw default allow outgoing >/dev/null 2>&1 || true
ufw default deny routed    >/dev/null 2>&1 || true
# SSH: allowed but RATE-LIMITED (ufw drops an IP after 6 hits / 30s — brute-force shield).
ufw limit OpenSSH >/dev/null 2>&1 || ufw limit 22/tcp >/dev/null 2>&1 || true
# Relay ports — NOT rate-limited (a relay must accept many concurrent connections).
ufw allow 80/tcp           >/dev/null 2>&1 || true   # ACME HTTP-01 challenge
ufw allow 443/tcp          >/dev/null 2>&1 || true   # relay HTTPS — the brokered traffic
ufw allow "$QUIC_PORT"/udp >/dev/null 2>&1 || true   # QUIC relay + address discovery
ufw logging low            >/dev/null 2>&1 || true
ufw --force enable         >/dev/null 2>&1 || true
# fail2ban bans IPs that brute-force SSH (relay traffic is legit, never banned).
systemctl enable --now fail2ban >/dev/null 2>&1 || true

# ── SSH: key-only login (passwords disabled) — WITH a lockout safety gate ──
echo "==> 6b/7  SSH hardening (key-only login)"
KEYS_FOUND=0
for f in /root/.ssh/authorized_keys /home/*/.ssh/authorized_keys; do
  [ -s "$f" ] && grep -qE '^(ssh-|ecdsa-|sk-)' "$f" 2>/dev/null && { KEYS_FOUND=1; break; }
done
if [ "$KEYS_FOUND" -eq 1 ]; then
  # Force key-only across the main config AND any drop-ins. Cloud images often
  # ship `PasswordAuthentication yes` in 50-cloud-init.conf, and sshd is
  # first-match-wins, so we must neutralise every occurrence, not just add ours.
  for cfg in /etc/ssh/sshd_config /etc/ssh/sshd_config.d/*.conf; do
    [ -f "$cfg" ] || continue
    sed -i -E 's/^#?[[:space:]]*PasswordAuthentication[[:space:]]+.*/PasswordAuthentication no/I' "$cfg"
    sed -i -E 's/^#?[[:space:]]*KbdInteractiveAuthentication[[:space:]]+.*/KbdInteractiveAuthentication no/I' "$cfg"
    sed -i -E 's/^#?[[:space:]]*ChallengeResponseAuthentication[[:space:]]+.*/ChallengeResponseAuthentication no/I' "$cfg"
  done
  cat > /etc/ssh/sshd_config.d/99-hey-hardening.conf <<'SSHEOF'
# Hey relay — key-only SSH. Public-key auth ONLY; passwords fully disabled.
PubkeyAuthentication yes
PasswordAuthentication no
KbdInteractiveAuthentication no
ChallengeResponseAuthentication no
PermitEmptyPasswords no
PermitRootLogin prohibit-password
SSHEOF
  # Validate BEFORE applying; reload (not restart) so the live session never drops.
  if sshd -t 2>/dev/null; then
    systemctl reload ssh 2>/dev/null || systemctl reload sshd 2>/dev/null || true
    echo "    SSH is now KEY-ONLY (password login disabled)."
  else
    echo "    ! sshd config test FAILED — left unchanged-reload skipped (your session is safe). Run: sshd -t"
  fi
else
  echo "    SKIPPED: no SSH public key found in authorized_keys."
  echo "    Disabling passwords now would LOCK YOU OUT, so this step was skipped."
  echo "    Add your key first (from your laptop):  ssh-copy-id root@THIS_SERVER_IP"
  echo "    then re-run the script, or apply manually once the key is in place."
fi

echo "==> 7/7  done"
echo
echo "================================================================"
echo "  Hey mesh hub is running."
echo "    status:   systemctl status iroh-relay"
echo "    logs:     journalctl -u iroh-relay -f"
echo
echo "  Paste THIS into Hey on BOTH phones"
echo "  (Profile -> Connection -> Relay server), then fully reopen Hey:"
echo
echo "        https://$HOST"
echo
echo "  (First Let's Encrypt cert takes ~30-60s — watch the logs.)"
echo "================================================================"
