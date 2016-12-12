set -ex

test_gen() {
    echo 'extern crate volatile_register;' > $td/src/lib.rs
    cargo run $flags --release -- -i $td/STM32F30x.svd $1 >> $td/src/lib.rs
    cargo build $flags --manifest-path $td/Cargo.toml
}

test_mode() {
    # test crate
    cargo init --name foo $td
    echo 'volatile-register = "0.1.0"' >> $td/Cargo.toml

    curl -L \
         https://raw.githubusercontent.com/posborne/cmsis-svd/python-0.4/data/STMicro/STM32F30x.svd \
         > $td/STM32F30x.svd

    # test the library
    cargo build $flags
    cargo build $flags --release

    # test the generated code
    test_gen
    test_gen dbgmcu
    test_gen gpioa
    test_gen gpioc
    test_gen i2c1
    test_gen rcc
    test_gen spi1
    test_gen tim2
    test_gen tim3
    test_gen tim6
}

deploy_mode() {
    cargo rustc $flags --release --bin svd2rust -- -C lto
}

run() {
    flags="--target $TARGET"

    case $TRAVIS_OS_NAME in
        linux)
            td=$(mktemp -d)
            ;;
        osx)
            td=$(mktemp -d -t tmp)
            ;;
    esac

    if [ -z $TRAVIS_TAG ]; then
        test_mode
    else
        deploy_mode
    fi
}

run
