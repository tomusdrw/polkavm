#!/usr/bin/env bash

set -euo pipefail

cd "${0%/*}/"

source build-common.sh

BUILD_WASM=0
BUILD_CKBVM=0
BUILD_NATIVE_X86_64=0
BUILD_NATIVE_X86=0

if [ "${BUILD_BENCHMARKS_INSTALL_ALL_TOOLCHAINS:-}" == "1" ]; then
    rustup target add wasm32-unknown-unknown
    rustup target add riscv64imac-unknown-none-elf
    if [ ! -d "/tmp/solana-platform-tools-1.39" ]; then
        echo "Downloading Solana platform tools..."
        curl -Lo /tmp/platform-tools-linux-x86_64-1.39.tar.bz2 'https://github.com/anza-xyz/platform-tools/releases/download/v1.39/platform-tools-linux-x86_64.tar.bz2'
        mkdir -p /tmp/solana-platform-tools-1.39
        tar -C /tmp/solana-platform-tools-1.39 -xf /tmp/platform-tools-linux-x86_64-1.39.tar.bz2
    fi

    export SOLANA_PLATFORM_TOOLS_DIR='/tmp/solana-platform-tools-1.39'
fi

if [[ "$(rustup target list --installed)" =~ "wasm32-unknown-unknown" ]]; then
    BUILD_WASM=1
else
    echo "WARN: the wasm32-unknown-unknown target is not installed; WASM binaries won't be built!"
    echo "      You can add it with: rustup target add wasm32-unknown-unknown"
fi

if [[ "$(rustup target list --installed)" =~ "riscv64imac-unknown-none-elf" ]]; then
    BUILD_CKBVM=1
else
    echo "WARN: the riscv64imac-unknown-none-elf target is not installed; CKBVM binaries won't be built!"
    echo "      You can add it with: rustup target add riscv64imac-unknown-none-elf"
fi

if [[ "$(rustc --print cfg)" =~ "target_os=\"linux\"" ]]; then
    if [[ "$(rustc --print cfg)" =~ "target_arch=\"x86_64\"" ]]; then
        BUILD_NATIVE_X86_64=1
        if [[ "$(rustup target list --installed)" =~ "i686-unknown-linux-gnu" ]]; then
            BUILD_NATIVE_X86=1
        fi
    fi
fi

if [ "${SOLANA_PLATFORM_TOOLS_DIR:-}" == "" ]; then
    echo "WARN: 'SOLANA_PLATFORM_TOOLS_DIR' is not set; Solana eBPF binaries won't be built!"
    case "$OSTYPE" in
        linux*)
            echo "      You can set it up like this:"
            echo "        $ curl -Lo platform-tools-linux-x86_64.tar.bz2 'https://github.com/anza-xyz/platform-tools/releases/download/v1.39/platform-tools-linux-x86_64.tar.bz2'"
            echo "        $ mkdir -p /tmp/solana-platform-tools"
            echo "        $ tar -C /tmp/solana-platform-tools -xf platform-tools-linux-x86_64.tar.bz2"
            echo "        $ export SOLANA_PLATFORM_TOOLS_DIR='/tmp/solana-platform-tools'"
            echo ""
        ;;
    esac
fi

build_polkavm() {
    echo "> Building: '$1' (polkavm, 32-bit)"

    RUSTFLAGS="$extra_flags" cargo build  \
        -Z build-std=core,alloc \
        --target "$PWD/../crates/polkavm-linker/riscv32emac-unknown-none-polkavm.json" \
        -q --release --bin $1 -p $1

    pushd ..

    cargo run -q -p polkatool link \
        --run-only-if-newer guest-programs/target/riscv32emac-unknown-none-polkavm/release/$1 \
        -o guest-programs/target/riscv32emac-unknown-none-polkavm/release/$1.polkavm

    popd

    echo "> Building: '$1' (polkavm, 64-bit)"

    RUSTFLAGS="$extra_flags" cargo build  \
        -Z build-std=core,alloc \
        --target "$PWD/../crates/polkavm-linker/riscv64emac-unknown-none-polkavm.json" \
        -q --release --bin $1 -p $1

    pushd ..

    cargo run -q -p polkatool link \
        --run-only-if-newer guest-programs/target/riscv64emac-unknown-none-polkavm/release/$1 \
        -o guest-programs/target/riscv64emac-unknown-none-polkavm/release/$1.polkavm

    popd
}

function build_benchmark() {
    extra_flags="${extra_flags:-}"

    # Unconditional build:
    build_polkavm $1

    if [ "${BUILD_WASM}" == "1" ]; then
        echo "> Building: '$1' (wasm)"
        RUSTFLAGS="-C target-cpu=mvp -C target-feature=-sign-ext $extra_flags" cargo build -q --target=wasm32-unknown-unknown --release --bin $1 -p $1
    fi

    if [ "${BUILD_NATIVE_X86_64}" == "1" ]; then
        echo "> Building: '$1' (native, x86_64)"
        RUSTFLAGS="$extra_flags" cargo build -q --target=x86_64-unknown-linux-gnu --release --lib -p $1
    fi

    if [ "${BUILD_NATIVE_X86}" == "1" ]; then
        echo "> Building: '$1' (native, i686)"
        RUSTFLAGS="$extra_flags" cargo build -q --target=i686-unknown-linux-gnu --release --lib -p $1
    fi

    if [ "${BUILD_CKBVM}" == "1" ]; then
        echo "> Building: '$1' (CKB VM)"
        RUSTFLAGS="$extra_flags -C target-feature=+zba,+zbb,+zbc,+zbs -C link-arg=-s --cfg=target_ckb_vm" cargo build -q --target=riscv64imac-unknown-none-elf --release --bin $1 -p $1
    fi

    if [ "${SOLANA_PLATFORM_TOOLS_DIR:-}" != "" ]; then
        echo "> Building: '$1' (Solana eBPF)"
        sed -i "s/version = 4/version = 3/" Cargo.lock
        CARGO_TARGET_SBF_SOLANA_SOLANA_LINKER=$SOLANA_PLATFORM_TOOLS_DIR/llvm/bin/lld \
        PATH=$PATH:$SOLANA_PLATFORM_TOOLS_DIR/rust/bin:$SOLANA_PLATFORM_TOOLS_DIR/llvm/bin \
        LD_LIBRARY_PATH=$SOLANA_PLATFORM_TOOLS_DIR/rust/lib:$SOLANA_PLATFORM_TOOLS_DIR/llvm/lib \
        RUSTC=$SOLANA_PLATFORM_TOOLS_DIR/rust/bin/rustc \
        RUSTFLAGS="-C link-arg=-e -C link-arg=__solana_entry_point -C link-arg=-T.cargo/solana.ld" \
        $SOLANA_PLATFORM_TOOLS_DIR/rust/bin/cargo build --target=sbf-solana-solana --release -Zbuild-std=std,panic_abort --lib -p $1
        sed -i "s/version = 3/version = 4/" Cargo.lock
    fi
}

build_benchmark "bench-minimal"
build_benchmark "bench-pinky"
build_benchmark "bench-prime-sieve"
build_benchmark "bench-memset"
