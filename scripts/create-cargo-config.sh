#!/bin/bash
set -e

# Cross-compilation script for Rust applications using libudev ans libssl
# Run this on macOS to cross-compile to Linux

# Configuration
TARGET="x86_64-unknown-linux-gnu"
SYSROOT="$(pwd)/.cross-sysroot"  # Adjust this path as needed

echo "=== Creating cross compilation config ==="
echo "Target: ${TARGET}"
echo "Sysroot: ${SYSROOT}"
echo ""

# Check if the target is installed
if ! rustup target list --installed | grep -q "${TARGET}"; then
    echo "Installing Rust target: ${TARGET}"
    rustup target add "${TARGET}"
fi

# Check if a cross-compiler linker is available
if ! command -v x86_64-linux-gnu-gcc &> /dev/null; then
    echo "ERROR: x86_64-linux-gnu-gcc not found!"
    echo ""
    echo "You need to install a cross-compiler toolchain for linking."
    echo ""
    echo "Option 1 - Install via Homebrew (Recommended):"
    echo "  brew tap messense/macos-cross-toolchains"
    echo "  brew install x86_64-unknown-linux-gnu"
    echo ""
    echo "Option 2 - Use zigbuild (alternative):"
    echo "  cargo install cargo-zigbuild"
    echo "  Then use: cargo zigbuild --target x86_64-unknown-linux-gnu"
    echo ""
    echo "Option 3 - Use cross (Docker-based):"
    echo "  cargo install cross"
    echo "  Then use: cross build --target x86_64-unknown-linux-gnu"
    exit 1
fi

echo "✓ Found cross-compiler: $(which x86_64-linux-gnu-gcc)"
echo ""

# Check if sysroot exists
if [ ! -d "${SYSROOT}" ]; then
    echo "ERROR: Sysroot directory not found: ${SYSROOT}"
    echo ""
    echo "You need to set up a sysroot with libudev installed."
    echo "This typically involves extracting packages from a Linux distribution"
    echo "or using a Docker container to prepare the necessary libraries."
    echo ""
    echo "Example: Create a sysroot from Ubuntu packages:"
    echo "  mkdir -p ${SYSROOT}"
    echo "  # Then extract libudev-dev and dependencies to ${SYSROOT}"
    exit 1
fi

# Verify libudev pkgconfig file exists in sysroot
PKGCONFIG_PATH="${SYSROOT}/usr/lib/pkgconfig:${SYSROOT}/usr/lib/x86_64-linux-gnu/pkgconfig:${SYSROOT}/usr/share/pkgconfig"
if ! ls ${SYSROOT}/usr/lib/pkgconfig/libudev.pc ${SYSROOT}/usr/lib/x86_64-linux-gnu/pkgconfig/libudev.pc ${SYSROOT}/usr/share/pkgconfig/libudev.pc 2>/dev/null | grep -q .; then
    echo "WARNING: libudev.pc not found in sysroot!"
    echo "Searched in: ${PKGCONFIG_PATH}"
    echo ""
fi

# Set up pkg-config environment variables for cross-compilation
# These are required by libudev-sys as documented in the README
export PKG_CONFIG_DIR=""
export PKG_CONFIG_LIBDIR="${PKGCONFIG_PATH}"
export PKG_CONFIG_SYSROOT_DIR="${SYSROOT}"
export PKG_CONFIG_ALLOW_CROSS=1

# Add include paths for OpenSSL and other libraries
# OpenSSL headers are split between /usr/include and /usr/include/x86_64-linux-gnu
export CFLAGS="-I${SYSROOT}/usr/include -I${SYSROOT}/usr/include/x86_64-linux-gnu"
export CXXFLAGS="-I${SYSROOT}/usr/include -I${SYSROOT}/usr/include/x86_64-linux-gnu"
export CPPFLAGS="-I${SYSROOT}/usr/include -I${SYSROOT}/usr/include/x86_64-linux-gnu"

# Set library paths
export LDFLAGS="-L${SYSROOT}/usr/lib/x86_64-linux-gnu -L${SYSROOT}/lib/x86_64-linux-gnu"

# For OpenSSL specifically (if using openssl-sys crate)
export OPENSSL_DIR="${SYSROOT}/usr"
export OPENSSL_LIB_DIR="${SYSROOT}/usr/lib/x86_64-linux-gnu"
export OPENSSL_INCLUDE_DIR="${SYSROOT}/usr/include"

# Linker configuration
# We need to use the system linker with GNU-compatible arguments
# The default rust-lld doesn't support the same arguments as GNU ld
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="x86_64-linux-gnu-gcc"

RUSTFLAGS="-C linker=x86_64-linux-gnu-gcc"
RUSTFLAGS="${RUSTFLAGS} -L${SYSROOT}/usr/lib/x86_64-linux-gnu"
RUSTFLAGS="${RUSTFLAGS} -L${SYSROOT}/lib/x86_64-linux-gnu"
RUSTFLAGS="${RUSTFLAGS} -C link-arg=-Wl,-rpath-link,${SYSROOT}/lib/x86_64-linux-gnu"
RUSTFLAGS="${RUSTFLAGS} -C link-arg=-Wl,-rpath-link,${SYSROOT}/usr/lib/x86_64-linux-gnu"
RUSTFLAGS="${RUSTFLAGS} -C link-arg=--sysroot=${SYSROOT}"

export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS="${RUSTFLAGS}"

echo "Environment variables set:"
echo "  PKG_CONFIG_DIR=${PKG_CONFIG_DIR}"
echo "  PKG_CONFIG_LIBDIR=${PKG_CONFIG_LIBDIR}"
echo "  PKG_CONFIG_SYSROOT_DIR=${PKG_CONFIG_SYSROOT_DIR}"
echo "  PKG_CONFIG_ALLOW_CROSS=${PKG_CONFIG_ALLOW_CROSS}"
echo "  OPENSSL_DIR=${OPENSSL_DIR}"
echo "  OPENSSL_LIB_DIR=${OPENSSL_LIB_DIR}"
echo "  OPENSSL_INCLUDE_DIR=${OPENSSL_INCLUDE_DIR}"
echo "  CFLAGS=${CFLAGS}"
echo ""

echo "=== Generating .cargo/config.toml ==="

# Create .cargo directory if it doesn't exist
mkdir -p .cargo

RUSTFLAGS_ARRAY="[
    \"-C\", \"linker=x86_64-linux-gnu-gcc\",
    \"-L\", \"${SYSROOT}/usr/lib/x86_64-linux-gnu\",
    \"-L\", \"${SYSROOT}/lib/x86_64-linux-gnu\",
    \"-C\", \"link-arg=-Wl,-rpath-link,${SYSROOT}/lib/x86_64-linux-gnu\",
    \"-C\", \"link-arg=-Wl,-rpath-link,${SYSROOT}/usr/lib/x86_64-linux-gnu\",
    \"-C\", \"link-arg=--sysroot=${SYSROOT}\",
]"

# Generate config.toml
cat > .cargo/config.toml <<EOF
# Auto-generated Cargo configuration for cross-compilation
# Generated by create-cargo-config.sh on $(date)
[build]
target = "${TARGET}"

[target.${TARGET}]
# Use the cross-compiler linker instead of rust-lld
linker = "x86_64-linux-gnu-gcc"

# Pass sysroot and library paths to the linker
rustflags = ${RUSTFLAGS_ARRAY}

[env]
# pkg-config configuration for cross-compilation
PKG_CONFIG_DIR = ""
PKG_CONFIG_LIBDIR = "${PKG_CONFIG_LIBDIR}"
PKG_CONFIG_SYSROOT_DIR = "${PKG_CONFIG_SYSROOT_DIR}"
PKG_CONFIG_ALLOW_CROSS = "1"

# OpenSSL configuration
OPENSSL_DIR = "${OPENSSL_DIR}"
OPENSSL_LIB_DIR = "${OPENSSL_LIB_DIR}"
OPENSSL_INCLUDE_DIR = "${OPENSSL_INCLUDE_DIR}"

# Compiler flags for include paths
CFLAGS = "${CFLAGS}"
CXXFLAGS = "${CXXFLAGS}"
CPPFLAGS = "${CPPFLAGS}"

# Linker flags for library paths
LDFLAGS = "${LDFLAGS}"
EOF

echo "✓ Created .cargo/config.toml"
echo ""
echo "You can now build with: cargo build --release"
echo "The environment variables will be automatically applied from the config file."
echo ""
echo "Note: Make sure your sysroot is at: ${SYSROOT}"
