#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IMAGE_NAME="hey/classifier_api"
PORT=5000

command_usage() {
  cat <<'EOF'
Usage: ./podman.sh [command]

Commands:
  build      Build the classifier API container image with Podman
  run        Run the classifier API container on port 5000
  pull       Pull the prebuilt image from a registry
  help       Show this help message
EOF
}

command_build() {
  echo "Building Podman image ${IMAGE_NAME} from ${ROOT_DIR}..."
  cd "${ROOT_DIR}"
  podman build -t "${IMAGE_NAME}" .
}

command_run() {
  echo "Running Podman container ${IMAGE_NAME} on port ${PORT}..."
  podman run --rm -p "${PORT}:5000" "${IMAGE_NAME}"
}

command_pull() {
  echo "Pulling image ${IMAGE_NAME}..."
  podman pull "${IMAGE_NAME}"
}

if [[ ${#@} -eq 0 ]]; then
  command_usage
  exit 0
fi

case "$1" in
  build)
    command_build
    ;;
  run)
    command_run
    ;;
  pull)
    command_pull
    ;;
  help|--help|-h)
    command_usage
    ;;
  *)
    echo "Unknown command: $1"
    command_usage
    exit 1
    ;;
esac
