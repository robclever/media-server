# setup

## Server details

- Purpose: host this repository's Custom Plex / Family Cinema application (not official Plex).
- Hardware: Raspberry Pi 3.
- OS image: Ubuntu Server 24.04.5 LTS, Raspberry Pi ARM64 (64-bit).
- Network: wired Ethernet, DHCP.
- Current LAN IP: `192.168.0.73` (reserve it in the router to keep it stable).
- Hostname configured: `custom-plex`.
- Login user: `rob`.
- Time zone: `America/Chicago`.
- SSH key on this Mac: `/Users/robclever/.ssh/custom_plex_pi`.
- SSH uses public-key authentication; password login and root SSH login are disabled.
- The `rob` account has passwordless sudo. No login password was configured.

## Current status — September 16, 2026

- Official Ubuntu image downloaded and SHA-256 verified.
- User imaged the SD card with Raspberry Pi Imager, copied both configuration files, connected Ethernet, and powered on the Pi.
- User successfully connected through Mac Terminal with SSH.
- Docker installation instructions provided below; completion has NOT yet been confirmed.
- Custom Plex has NOT yet been deployed or verified on the Pi.

## SD card preparation

The confirmed card was the approximately 196.9 GB removable USB device named
`Mass Storage Device Media` / `Storage Device`. It appeared as `/dev/disk8`
during setup. Disk identifiers can change: always re-identify the card before
writing. The 500.1 GB `My Passport` drive was NOT the target.

Image filename:

```text
ubuntu-24.04.5-preinstalled-server-arm64+raspi.img.xz
```

Verified compressed-image SHA-256:

```text
b23371a5c8d02f612c26e7c18cacb81c5a48f968c7c66a2645391320aa94351e
```

Download source: https://cdimage.ubuntu.com/releases/24.04/release/

Temporary preparation directory on this Mac:
`/private/tmp/custom-plex-pi-image`. These files are temporary and may be removed
by macOS; the persistent SSH key is stored separately in `~/.ssh`.

Manual imaging procedure:

1. Install Raspberry Pi Imager from https://www.raspberrypi.com/software/.
2. Select Raspberry Pi 3, then **Use custom** and select
   `/private/tmp/custom-plex-pi-image/ubuntu-24.04.5-preinstalled-server-arm64+raspi.img`.
3. Select the confirmed 196.9 GB SD card. Writing erases its existing contents.
4. Skip Imager OS customization when using the configuration below. Write the
   image and allow verification to finish.
5. Remove and reinsert the card into the Mac so `system-boot` mounts.
6. Copy the prepared configuration files in Mac Terminal:

```sh
cp /private/tmp/custom-plex-pi-image/user-data /Volumes/system-boot/user-data
cp /private/tmp/custom-plex-pi-image/network-config /Volumes/system-boot/network-config
sync
```

7. Eject `system-boot` in Finder, insert the card into the powered-off Pi,
   connect Ethernet to the router, and connect power. Allow several minutes for
   cloud-init to finish the first boot.

### Boot configuration reference

The `user-data` file used the configuration below, with the contents of
`/Users/robclever/.ssh/custom_plex_pi.pub` substituted for the public-key
placeholder. Never copy the private key onto the SD card or into this repository.

```yaml
#cloud-config
hostname: custom-plex
manage_etc_hosts: true
timezone: America/Chicago
users:
  - name: rob
    gecos: Rob
    groups: [adm, sudo]
    shell: /bin/bash
    sudo: ALL=(ALL) NOPASSWD:ALL
    lock_passwd: true
    ssh_authorized_keys:
      - REPLACE_WITH_CONTENTS_OF_custom_plex_pi.pub
ssh_pwauth: false
disable_root: true
package_update: false
package_upgrade: false
runcmd:
  - [systemctl, enable, --now, ssh]
```

The `network-config` file enables DHCP on Ethernet:

```yaml
version: 2
ethernets:
  ethernet:
    match:
      name: "e*"
    dhcp4: true
    optional: true
```

## Connect from Mac Terminal

```sh
ssh -i /Users/robclever/.ssh/custom_plex_pi rob@192.168.0.73
```

Accept the first-connection host-key prompt after confirming the address belongs
to the Pi. The user confirmed this command connected successfully.

`custom-plex.local` did not resolve during setup; use the IP address. If it
changes, find `custom-plex` or `ubuntu` in the router's connected-device list.

Optional initial checks, run in the Pi SSH session:

```sh
hostname
uname -m
cat /etc/os-release
cloud-init status
free -h
df -h /
```

The expected architecture is `aarch64`. Wait for cloud-init to finish before
installing packages.

## Install Docker Engine and Compose

Run these commands INSIDE the Pi SSH session, not on the Mac. These instructions
use Docker's Ubuntu repository because this Pi runs Ubuntu; the README's Debian
repository instructions apply to Raspberry Pi OS instead.

Official instructions: https://docs.docker.com/engine/install/ubuntu/

```sh
sudo apt update &&
sudo apt install -y ca-certificates curl &&
sudo install -m 0755 -d /etc/apt/keyrings &&
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg \
  -o /etc/apt/keyrings/docker.asc &&
sudo chmod a+r /etc/apt/keyrings/docker.asc
```

Then configure the repository and install:

```sh
sudo tee /etc/apt/sources.list.d/docker.sources > /dev/null <<EOF_DOCKER
Types: deb
URIs: https://download.docker.com/linux/ubuntu
Suites: $(. /etc/os-release && echo "$VERSION_CODENAME")
Components: stable
Architectures: $(dpkg --print-architecture)
Signed-By: /etc/apt/keyrings/docker.asc
EOF_DOCKER

sudo apt update &&
sudo apt install -y docker-ce docker-ce-cli containerd.io \
  docker-buildx-plugin docker-compose-plugin &&
sudo systemctl enable --now docker
```

Verify:

```sh
uname -m
sudo docker compose version
sudo docker run --rm hello-world
```

Expected results: `aarch64`, a Compose version, and `Hello from Docker!`.
Use `sudo` consistently for Docker commands; no Docker group membership has
been configured.

## Next: deploy Custom Plex

After Docker verification succeeds, follow README.md under **Deploy from source
(no published image needed)**. No published registry image is required. Use the
SSH key and actual user/address above for transfers, and use `sudo docker`
for Docker commands on this Pi.

Deployment remains pending, including source transfer, container build/start,
parents-password setup, health check, movie import, and actual TV playback tests.
The intended browser address is `http://192.168.0.73:8080` once deployment is
complete. Keep the service on the trusted home network; do not forward port
8080 to the internet.

## Setup troubleshooting observed

- Automated raw SD-card writes failed with `Operation not permitted`, even
  after administrator authorization. The installed app is displayed as
  `ChatGPT`, with bundle identifier `com.openai.codex`; the user reported it
  already had Full Disk Access. The underlying cause was not established.
  Manual Raspberry Pi Imager was used instead.
- SSH from the app to `192.168.0.73` failed with `No route to host`, while the
  same SSH connection succeeded in Mac Terminal. The cause was not established;
  use Mac Terminal for the remaining remote setup.
