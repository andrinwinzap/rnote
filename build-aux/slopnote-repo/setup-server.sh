#!/usr/bin/env bash
# Prepare the directory the Slopnote flatpak repo is served from, and the user the pipeline
# uploads it as. Run it on the server, as root.
#
# The server only ever serves files: the flatpak repo is built and signed in the pipeline, so
# neither flatpak nor the signing key is needed here.

set -euo pipefail

DEPLOY_USER="${DEPLOY_USER:-slopnote}"
DEPLOY_PATH="${DEPLOY_PATH:-/srv/slopnote}"

if [ "$(id -u)" -ne 0 ]; then
    echo "run this as root" >&2
    exit 1
fi

if ! id "$DEPLOY_USER" >/dev/null 2>&1; then
    # No shell beyond what rsync over ssh needs, no password, nothing else on the box.
    useradd --system --create-home --home-dir "/var/lib/$DEPLOY_USER" \
        --shell /bin/bash "$DEPLOY_USER"
    echo "created the user $DEPLOY_USER"
fi

mkdir -p "$DEPLOY_PATH"
chown "$DEPLOY_USER:$DEPLOY_USER" "$DEPLOY_PATH"
# The web server reads it, the deploy user writes it.
chmod 755 "$DEPLOY_PATH"

install -d -o "$DEPLOY_USER" -g "$DEPLOY_USER" -m 700 "/var/lib/$DEPLOY_USER/.ssh"
touch "/var/lib/$DEPLOY_USER/.ssh/authorized_keys"
chown "$DEPLOY_USER:$DEPLOY_USER" "/var/lib/$DEPLOY_USER/.ssh/authorized_keys"
chmod 600 "/var/lib/$DEPLOY_USER/.ssh/authorized_keys"

cat <<EOF

The directory is ready: $DEPLOY_PATH, owned by $DEPLOY_USER.

What is left:

  1. Put the public half of the deploy key in
     /var/lib/$DEPLOY_USER/.ssh/authorized_keys

  2. Point the web server at $DEPLOY_PATH, with nginx.conf in this directory as a
     starting point, and route your domain to it with traefik.

  3. Set the secrets in the GitHub repository, which README.md in this directory lists.

Nothing is served yet: the first run of the pipeline puts the repo there.
EOF
