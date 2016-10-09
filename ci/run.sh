set -ex

test_gen() {
    echo 'extern crate volatile_register;' > /tmp/foo/src/lib.rs
    cargo run --release -- -i /tmp/STM32F30x.svd $1 >> /tmp/foo/src/lib.rs
    cargo build --manifest-path /tmp/foo/Cargo.toml
}

main() {
    export LD_LIBRARY_PATH=$(rustc --print sysroot)/lib/rustlib/${1}/lib
    export USER=rust

    curl -L \
         https://raw.githubusercontent.com/posborne/cmsis-svd/master/data/STMicro/STM32F30x.svd \
         > /tmp/STM32F30x.svd

    # test the library
    cargo build --release

    # test repository
    cargo new /tmp/foo
    echo 'volatile-register = "0.1.0"' >> /tmp/foo/Cargo.toml

    # test generated code
    test_gen
    test_gen dbgmcu
    test_gen gpioa
    test_gen i2c1
    test_gen rcc
    test_gen spi1
    test_gen tim6
}

main $1
