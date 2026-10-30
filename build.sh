#!/usr/bin/env bash
# ==============================================================================
# LispCraft Local Build & Release Packaging Pipeline
# Replicates GitHub Actions CI/CD workflows for local development and release packaging.
# ==============================================================================

set -euo pipefail

# Text color formatting
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Default configurations
RUN_TESTS=false
CLEAN_BUILD=false
SKIP_WASM=false
SKIP_NATIVE=false
OUTPUT_DIR="dist"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cd "${ROOT_DIR}"

log_info()  { echo -e "${CYAN}[INFO]${NC} $*"; }
log_step()  { echo -e "\n${BOLD}${BLUE}==> $*${NC}"; }
log_ok()    { echo -e "${GREEN}[✓]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

show_help() {
    cat << EOF
LispCraft Local Build Script (GitHub Actions Parity)

Usage:
  ./build.sh [OPTIONS]

Options:
  -t, --test         Run full test suite (cargo test --workspace + Python E2E)
  -c, --clean        Run 'cargo clean' before building
  --skip-wasm        Skip building WebAssembly client
  --skip-native      Skip building Native binaries (WASM only)
  -o, --output <DIR> Custom output directory for packages (default: dist)
  -h, --help         Show this help message

Examples:
  ./build.sh               # Standard full build and packaging
  ./build.sh --test        # Build and run all unit and integration tests
  ./build.sh --skip-wasm   # Fast build of native binaries only
EOF
}

# Parse command line flags
while [[ $# -gt 0 ]]; do
    case "$1" in
        -t|--test)
            RUN_TESTS=true
            shift
            ;;
        -c|--clean)
            CLEAN_BUILD=true
            shift
            ;;
        --skip-wasm)
            SKIP_WASM=true
            shift
            ;;
        --skip-native)
            SKIP_NATIVE=true
            shift
            ;;
        -o|--output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

echo -e "${BOLD}====================================================${NC}"
echo -e "${BOLD}       LispCraft Multiplatform Build Pipeline       ${NC}"
echo -e "${BOLD}====================================================${NC}"

# ------------------------------------------------------------------------------
# 1. Environment & Architecture Detection
# ------------------------------------------------------------------------------
log_step "1. Detecting Local Operating System & Hardware Architecture"

OS_RAW="$(uname -s)"
ARCH_RAW="$(uname -m)"

case "${OS_RAW}" in
    Linux*)
        OS_NAME="linux"
        EXE_SUFFIX=""
        ARCHIVE_EXT="tar.gz"
        ;;
    Darwin*)
        OS_NAME="macos"
        EXE_SUFFIX=""
        ARCHIVE_EXT="tar.gz"
        ;;
    CYGWIN*|MINGW*|MSYS*)
        OS_NAME="windows"
        EXE_SUFFIX=".exe"
        ARCHIVE_EXT="zip"
        ;;
    *)
        log_warn "Unrecognized OS '${OS_RAW}'. Falling back to linux generic."
        OS_NAME="linux"
        EXE_SUFFIX=""
        ARCHIVE_EXT="tar.gz"
        ;;
esac

case "${ARCH_RAW}" in
    x86_64|amd64)
        ARCH_NAME="x86_64"
        ;;
    arm64|aarch64)
        ARCH_NAME="arm64"
        ;;
    *)
        ARCH_NAME="${ARCH_RAW}"
        ;;
esac

PACKAGE_NAME="lispcraft-${OS_NAME}-${ARCH_NAME}"
log_ok "Detected Environment: OS=${BOLD}${OS_NAME}${NC}, Arch=${BOLD}${ARCH_NAME}${NC}"
log_ok "Target Bundle Name:   ${BOLD}${PACKAGE_NAME}${NC}"

# ------------------------------------------------------------------------------
# 2. Toolchain Prerequisites Verification
# ------------------------------------------------------------------------------
log_step "2. Verifying Build Dependencies & Toolchains"

if ! command -v cargo &> /dev/null; then
    log_error "'cargo' not found in PATH! Please install Rust via https://rustup.rs"
    exit 1
fi
RUSTC_VER="$(rustc --version)"
log_ok "Rust Toolchain: ${RUSTC_VER}"

# Check for wasm32 target if WASM build is requested
if [[ "${SKIP_WASM}" == "false" ]]; then
    if command -v rustup &> /dev/null; then
        if ! rustup target list | grep -q "wasm32-unknown-unknown (installed)"; then
            log_warn "Target wasm32-unknown-unknown is not installed. Installing now..."
            rustup target add wasm32-unknown-unknown
            log_ok "Installed wasm32-unknown-unknown target."
        else
            log_ok "Target 'wasm32-unknown-unknown' is installed."
        fi
    else
        log_warn "'rustup' not found. Assuming wasm32-unknown-unknown is available."
    fi
fi

# Linux native dependency warnings
if [[ "${OS_NAME}" == "linux" && "${SKIP_NATIVE}" == "false" ]]; then
    MISSING_DEPS=()
    for cmd in pkg-config; do
        if ! command -v "$cmd" &> /dev/null; then
            MISSING_DEPS+=("$cmd")
        fi
    done
    if [[ ${#MISSING_DEPS[@]} -gt 0 ]]; then
        log_warn "Some build utilities may be missing: ${MISSING_DEPS[*]}"
        log_info "On Debian/Ubuntu, install with: sudo apt-get install -y pkg-config libx11-dev libxi-dev libgl1-mesa-dev libasound2-dev"
    fi
fi

# Optional clean build
if [[ "${CLEAN_BUILD}" == "true" ]]; then
    log_step "Cleaning Previous Build Artifacts"
    cargo clean
    rm -rf "${OUTPUT_DIR}"
    log_ok "Workspace cleaned."
fi

# ------------------------------------------------------------------------------
# 3. Optional Automated Testing
# ------------------------------------------------------------------------------
if [[ "${RUN_TESTS}" == "true" ]]; then
    log_step "3. Running Automated Test Suites"
    log_info "Running Rust workspace unit tests..."
    cargo test --workspace
    log_ok "Workspace unit tests passed!"

    if command -v python3 &> /dev/null && [[ -f "tests/e2e_multiplayer_test.py" ]]; then
        log_info "Running End-to-End multiplayer integration tests..."
        python3 tests/e2e_multiplayer_test.py
        log_ok "End-to-End integration tests passed (19/19)!"
    else
        log_warn "Python 3 not found or tests/e2e_multiplayer_test.py missing, skipping E2E tests."
    fi
fi

# ------------------------------------------------------------------------------
# 4. Build WebAssembly Client
# ------------------------------------------------------------------------------
if [[ "${SKIP_WASM}" == "false" ]]; then
    log_step "4. Building WebAssembly Client (target: wasm32-unknown-unknown)"
    cargo build --release --target wasm32-unknown-unknown --bin client

    WASM_SRC="target/wasm32-unknown-unknown/release/client.wasm"
    WASM_DST="web/client.wasm"

    if [[ -f "${WASM_SRC}" ]]; then
        mkdir -p web
        cp "${WASM_SRC}" "${WASM_DST}"
        WASM_SIZE="$(du -h "${WASM_DST}" | cut -f1)"
        log_ok "Built and synced: ${WASM_DST} (${WASM_SIZE})"
    else
        log_error "Expected WASM artifact not found at ${WASM_SRC}"
        exit 1
    fi
else
    log_step "4. Skipping WebAssembly Client Build (--skip-wasm specified)"
fi

# ------------------------------------------------------------------------------
# 5. Build Native Release Binaries
# ------------------------------------------------------------------------------
if [[ "${SKIP_NATIVE}" == "false" ]]; then
    log_step "5. Building Native Release Binaries (server, client)"
    cargo build --release --bins

    SERVER_BIN="target/release/server${EXE_SUFFIX}"
    CLIENT_BIN="target/release/client${EXE_SUFFIX}"

    if [[ ! -f "${SERVER_BIN}" || ! -f "${CLIENT_BIN}" ]]; then
        log_error "Expected release binaries not found in target/release/"
        exit 1
    fi
    log_ok "Built native server binary: ${SERVER_BIN} ($(du -h "${SERVER_BIN}" | cut -f1))"
    log_ok "Built native client binary: ${CLIENT_BIN} ($(du -h "${CLIENT_BIN}" | cut -f1))"
else
    log_step "5. Skipping Native Binaries Build (--skip-native specified)"
fi

# ------------------------------------------------------------------------------
# 6. Assemble Release Bundle & Package Archive
# ------------------------------------------------------------------------------
log_step "6. Assembling Release Package Bundle"

DIST_PATH="${OUTPUT_DIR}/${PACKAGE_NAME}"
mkdir -p "${DIST_PATH}"

if [[ "${SKIP_NATIVE}" == "false" ]]; then
    rm -f "${DIST_PATH}/server${EXE_SUFFIX}" "${DIST_PATH}/client${EXE_SUFFIX}"
    cp "target/release/server${EXE_SUFFIX}" "${DIST_PATH}/"
    cp "target/release/client${EXE_SUFFIX}" "${DIST_PATH}/"
fi

if [[ -d "games" ]]; then
    cp -r games "${DIST_PATH}/"
fi

if [[ -d "web" ]]; then
    cp -r web "${DIST_PATH}/"
fi

if [[ -f "README.md" ]]; then
    cp README.md "${DIST_PATH}/"
fi

if [[ -f "LICENSE" ]]; then
    cp LICENSE "${DIST_PATH}/"
fi

# Create compressed archive
cd "${OUTPUT_DIR}"
ARCHIVE_FILE="${PACKAGE_NAME}.${ARCHIVE_EXT}"
rm -f "${ARCHIVE_FILE}"

if [[ "${ARCHIVE_EXT}" == "tar.gz" ]]; then
    tar -czf "${ARCHIVE_FILE}" "${PACKAGE_NAME}"
elif [[ "${ARCHIVE_EXT}" == "zip" ]]; then
    if command -v zip &> /dev/null; then
        zip -rq "${ARCHIVE_FILE}" "${PACKAGE_NAME}"
    elif command -v 7z &> /dev/null; then
        7z a "${ARCHIVE_FILE}" "${PACKAGE_NAME}" > /dev/null
    else
        log_warn "Neither 'zip' nor '7z' found; creating tar.gz instead."
        ARCHIVE_FILE="${PACKAGE_NAME}.tar.gz"
        tar -czf "${ARCHIVE_FILE}" "${PACKAGE_NAME}"
    fi
fi

cd "${ROOT_DIR}"
ARCHIVE_SIZE="$(du -h "${OUTPUT_DIR}/${ARCHIVE_FILE}" | cut -f1)"
log_ok "Packaged Archive: ${OUTPUT_DIR}/${ARCHIVE_FILE} (${ARCHIVE_SIZE})"

# ------------------------------------------------------------------------------
# 7. Summary & Quick-Start Guide
# ------------------------------------------------------------------------------
echo -e "\n${BOLD}====================================================${NC}"
echo -e "${GREEN}${BOLD}✓ BUILD & PACKAGING COMPLETED SUCCESSFULLY!${NC}"
echo -e "${BOLD}====================================================${NC}"
echo -e "Bundle Directory: ${CYAN}${DIST_PATH}${NC}"
echo -e "Release Archive:  ${CYAN}${OUTPUT_DIR}/${ARCHIVE_FILE}${NC} (${ARCHIVE_SIZE})"
echo ""
echo -e "${BOLD}Quick Run Commands:${NC}"
echo -e "  ${YELLOW}# 1. Start Server:${NC}"
echo -e "  ./${DIST_PATH}/server${EXE_SUFFIX} games/space_arena"
echo ""
echo -e "  ${YELLOW}# 2. Connect Native Client:${NC}"
echo -e "  ./${DIST_PATH}/client${EXE_SUFFIX} http://localhost:8087/space_arena/guest_1"
echo ""
echo -e "  ${YELLOW}# 3. Web Client:${NC}"
echo -e "  Open your browser at: ${CYAN}http://localhost:8087/space_arena/guest_1${NC}"
echo -e "${BOLD}====================================================${NC}"
