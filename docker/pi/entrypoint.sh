#!/bin/sh
# Usage: entrypoint.sh --key-env <NAME> <pi args...>
# Reads the API key from the read-only file /run/solo/key into $NAME, then
# runs pi in /solo/outputs. The key is never printed.

if [ "$1" != "--key-env" ] || [ -z "$2" ]; then
    echo "entrypoint: expected '--key-env <NAME> <pi args...>'" >&2
    exit 64
fi
name=$2
shift 2

case "$name" in
    [A-Z]*) ;;
    *) echo "entrypoint: invalid key variable name" >&2; exit 64 ;;
esac
case "$name" in
    *[!A-Z0-9_]*) echo "entrypoint: invalid key variable name" >&2; exit 64 ;;
esac

keyfile=/run/solo/key
if [ ! -r "$keyfile" ] || [ ! -s "$keyfile" ]; then
    echo "entrypoint: key file $keyfile is missing or empty" >&2
    exit 66
fi
key=$(cat "$keyfile")
if [ -z "$key" ]; then
    echo "entrypoint: key file $keyfile is missing or empty" >&2
    exit 66
fi
export "$name=$key"
unset key

export HOME=/tmp
export PI_CODING_AGENT_DIR=/tmp/pi-agent
export PI_TELEMETRY=0
export PI_SKIP_VERSION_CHECK=1
export PI_OFFLINE=1
mkdir -p "$PI_CODING_AGENT_DIR"

cd /solo/outputs || exit 70
exec pi "$@"
