#!/bin/bash
set -e

# Script to create a Linux sysroot with libudev for cross-compilation
# This script uses Docker to extract the necessary libraries from Ubuntu

SYSROOT="$(pwd)/.cross-sysroot"
CONTAINER_NAME="sysroot-builder"
UBUNTU_VERSION="24.04"  # Change if needed

# Array of packages to install in the sysroot
# Add or remove packages as needed for your project
PACKAGES=(
    "libudev-dev"
    "libssl-dev"
    "libc6-dev"
)

echo "=== Creating Linux sysroot with libudev ==="
echo "Sysroot location: ${SYSROOT}"
echo "Packages to install: ${PACKAGES[*]}"
echo ""

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    echo "ERROR: Docker is not installed or not running"
    echo "Please install Docker Desktop for Mac from: https://www.docker.com/products/docker-desktop"
    exit 1
fi

# Remove sysroot directory
if [ -d "${SYSROOT}" ]; then
    echo "Removing old sysroot directory..."
    rm -rf "${SYSROOT}"
fi

# Create sysroot directory
echo "Creating sysroot directory..."
mkdir -p "${SYSROOT}"

# Start Ubuntu container
echo "Starting Ubuntu ${UBUNTU_VERSION} container..."
docker run --name "${CONTAINER_NAME}" --platform linux/amd64 -d ubuntu:${UBUNTU_VERSION} sleep infinity || {
    echo "Removing old container and creating new one..."
    docker rm -f "${CONTAINER_NAME}"
    docker run --name "${CONTAINER_NAME}" --platform linux/amd64 -d ubuntu:${UBUNTU_VERSION} sleep infinity
}

# Update and install libudev-dev in the container
echo "Installing libudev-dev and dependencies in container..."
docker exec "${CONTAINER_NAME}" bash -c "
    apt-get update && \
    apt-get install -y pkg-config ${PACKAGES[*]}
"

# List of essential files/directories to copy
echo "Copying files from container to sysroot..."

mkdir -p "${SYSROOT}/usr/lib/x86_64-linux-gnu/"
mkdir -p "${SYSROOT}/usr/share/"
mkdir -p "${SYSROOT}/lib/"

# Copy library directories
docker cp "${CONTAINER_NAME}:/usr/lib/x86_64-linux-gnu" "${SYSROOT}/usr/lib/"
docker cp "${CONTAINER_NAME}:/lib/x86_64-linux-gnu" "${SYSROOT}/lib/"

# Copy include files
docker cp "${CONTAINER_NAME}:/lib64" "${SYSROOT}/" 2>/dev/null || true
docker cp "${CONTAINER_NAME}:/usr/include" "${SYSROOT}/usr/"

# Copy pkgconfig files
docker cp "${CONTAINER_NAME}:/usr/lib/x86_64-linux-gnu/pkgconfig" "${SYSROOT}/usr/lib/x86_64-linux-gnu/" 2>/dev/null || true
docker cp "${CONTAINER_NAME}:/usr/share/pkgconfig" "${SYSROOT}/usr/share/" 2>/dev/null || true

# create symlinks for library directories (gcc doesnt check
# `/usr/lib/x86_64-linux-gnu` by default when sysroot is set)
ln -s "${SYSROOT}/usr/lib/x86_64-linux-gnu" "${SYSROOT}/usr/lib64"
ln -s "${SYSROOT}/usr/lib/x86_64-linux-gnu" "${SYSROOT}/lib64"

# Clean up container
echo "Cleaning up container..."
docker stop "${CONTAINER_NAME}"
docker rm "${CONTAINER_NAME}"

echo ""
echo "=== Sysroot created successfully! ==="
echo ""
echo "Sysroot location: ${SYSROOT}"
echo ""
echo "Verifying libudev installation..."

# Verify critical files exist
CRITICAL_FILES=(
    "${SYSROOT}/usr/lib/x86_64-linux-gnu/libudev.so"
    "${SYSROOT}/usr/lib/x86_64-linux-gnu/pkgconfig/libudev.pc"
    "${SYSROOT}/usr/include/libudev.h"
    "${SYSROOT}/usr/lib/x86_64-linux-gnu/libssl.so"
    "${SYSROOT}/usr/lib/x86_64-linux-gnu/pkgconfig/openssl.pc"
    "${SYSROOT}/usr/include/openssl/ssl.h"
)

STARTUP_FILES=(
    "${SYSROOT}/usr/lib/x86_64-linux-gnu/Scrt1.o"
    "${SYSROOT}/usr/lib/x86_64-linux-gnu/crti.o"
    "${SYSROOT}/usr/lib/x86_64-linux-gnu/crtn.o"
    "${SYSROOT}/lib/x86_64-linux-gnu/libc.so.6"
)

ALL_FOUND=true

echo "Checking critical library files..."
for file in "${CRITICAL_FILES[@]}"; do
    if [ -e "$file" ]; then
        echo "✓ Found: $file"
    else
        echo "✗ Missing: $file"
        ALL_FOUND=false
    fi
done

echo ""
echo "Checking startup object files..."
for file in "${STARTUP_FILES[@]}"; do
    if [ -e "$file" ]; then
        echo "✓ Found: $file"
    else
        echo "✗ Missing: $file"
        ALL_FOUND=false
    fi
done

echo ""
if [ "$ALL_FOUND" = true ]; then
    echo "✓ Sysroot is ready for cross-compilation!"
    echo ""
    echo "Next steps:"
    echo "1. Run the cross-compilation script with this sysroot"
    echo "2. Or set these environment variables in your shell:"
    echo ""
    echo "   export PKG_CONFIG_DIR=\"\""
    echo "   export PKG_CONFIG_LIBDIR=\"${SYSROOT}/usr/lib/x86_64-linux-gnu/pkgconfig:${SYSROOT}/usr/share/pkgconfig\""
    echo "   export PKG_CONFIG_SYSROOT_DIR=\"${SYSROOT}\""
    echo "   export PKG_CONFIG_ALLOW_CROSS=1"
    echo ""
    echo "   Then run: cargo build --target x86_64-unknown-linux-gnu"
else
    echo "✗ Some files are missing. The sysroot may be incomplete."
    exit 1
fi
