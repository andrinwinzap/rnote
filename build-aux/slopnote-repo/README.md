# Publishing Slopnote

Every push to `main` builds the flatpak and publishes it to a flatpak repo on the server, so that
`flatpak update` picks it up like anything from Flathub.

The pipeline is `.github/workflows/slopnote_publish.yml`.

Once it has run, the app is installed with:

```sh
flatpak remote-add --if-not-exists slopnote https://<your domain>/slopnote.flatpakrepo
flatpak install slopnote ch.andrinwinzap.Slopnote
```

## How a publish goes

1. The flatpak is built in the pipeline and its commit is signed there.
2. The repo metadata is written, static deltas and all, and signed as well.
3. The build is uploaded to `repo-<commit>` on the server, hardlinking everything that did not
   change against what is already being served, so an upload costs only what is new.
4. The `repo` symlink is swapped over in one step. Anyone updating at that moment sees either the
   build before or the build after, never half of each. The three builds behind it are kept, which
   is what rolling back needs.

The server only serves files. Neither flatpak nor the signing key is needed on it.

## Setting it up

### 1. The signing key

A flatpak repo is signed, and the client checks the signature against the key in the
`.flatpakrepo` file. Make a key for it, on a machine you trust, not on the server:

```sh
gpg --batch --passphrase '' --quick-generate-key \
    "Slopnote Repo <you@example.org>" rsa4096 sign never
gpg --list-secret-keys --keyid-format=long "Slopnote Repo"
```

The key has no passphrase, which is what `--passphrase ''` is for. There is nobody to type one in
the pipeline, and a passphrase on a key that sits in a secret next to the thing it unlocks is not
worth the trouble it causes.

Export the private half for the pipeline. It goes straight into the GitHub secret, so keep it off
disk if you can:

```sh
gpg --armor --export-secret-keys "Slopnote Repo"
```

Back up that key somewhere safe. Losing it means every client has to remove the remote and add it
again, since a repo signed by a new key is a different repo as far as flatpak is concerned.

### 2. The server

As root, with the defaults being the user `slopnote` and the directory `/srv/slopnote`:

```sh
./setup-server.sh
```

Make an SSH key for the pipeline to upload with, on your own machine:

```sh
ssh-keygen -t ed25519 -f slopnote-deploy -C "slopnote deploy" -N ""
```

Put `slopnote-deploy.pub` in `/var/lib/slopnote/.ssh/authorized_keys` on the server. Tightening it
to what the pipeline does is worth the line:

```
command="rrsync /srv/slopnote",restrict ssh-ed25519 AAAA... slopnote deploy
```

`restrict` turns off everything a deploy key has no business doing, and `rrsync` holds rsync to
that one directory. The publish step runs `ln`, `mv` and `rm` over SSH as well, so either drop the
`command=` part, or keep it and move those three into a small script the key is allowed to run.

### 3. nginx and traefik

`nginx.conf` in this directory serves `/srv/slopnote` on port 8081. The only thing in it that is
not ordinary is `disable_symlinks off`, without which nginx refuses to follow the symlink a
publish swaps over.

If nginx runs in docker behind traefik, the labels are:

```yaml
labels:
    - traefik.enable=true
    - traefik.http.routers.slopnote.rule=Host(`flatpak.example.org`)
    - traefik.http.routers.slopnote.entrypoints=websecure
    - traefik.http.routers.slopnote.tls.certresolver=letsencrypt
    - traefik.http.services.slopnote.loadbalancer.server.port=8081
```

If nginx runs on the host, the same thing as a file for traefik's file provider:

```yaml
http:
    routers:
        slopnote:
            rule: "Host(`flatpak.example.org`)"
            entryPoints: [websecure]
            service: slopnote
            tls:
                certResolver: letsencrypt
    services:
        slopnote:
            loadBalancer:
                servers:
                    - url: "http://127.0.0.1:8081"
```

### 4. The secrets

In the GitHub repository, under Settings → Secrets and variables → Actions.

Secrets:

| Name | What goes in it |
| --- | --- |
| `FLATPAK_GPG_PRIVATE_KEY` | the private key from step 1, armored |
| `DEPLOY_SSH_KEY` | the private half of the deploy key from step 2 |
| `DEPLOY_KNOWN_HOSTS` | `ssh-keyscan <your server>`, so the upload cannot be sent elsewhere |
| `DEPLOY_HOST` | the server the upload goes to |
| `DEPLOY_USER` | `slopnote`, unless it was changed |
| `DEPLOY_PATH` | `/srv/slopnote`, unless it was changed |

Variables:

| Name | What goes in it |
| --- | --- |
| `SLOPNOTE_REPO_URL` | `https://flatpak.example.org`, with no trailing slash |

The host, user and path are secrets rather than variables only because they say where the server is
and nothing is gained by printing them in a public log.

## Things worth knowing

A build from nothing takes the better part of an hour, most of it compiling dependencies. The
pipeline keeps what flatpak-builder caches between runs, so a run that only changes rnote's own
code is far shorter.

Flatpak will not install an app that has no release in its metainfo on some setups. Slopnote keeps
upstream's, so the newest release listed there is whatever upstream last put in it, which says
nothing about which build is installed.
