set -ex

test_gen() {
    echo 'extern crate volatile_register;' > $td/src/lib.rs
    cargo run $flags --release -- -i $td/$svd $1 >> $td/src/lib.rs
    cargo build $flags --manifest-path $td/Cargo.toml
}

test_mode() {
    # test crate
    cargo init --name foo $td
    echo 'volatile-register = "0.1.0"' >> $td/Cargo.toml

    curl -L \
         https://raw.githubusercontent.com/posborne/cmsis-svd/python-0.4/data/STMicro/STM32F30x.svd \
         > $td/STM32F30x.svd

    curl -L \
         https://raw.githubusercontent.com/posborne/cmsis-svd/python-0.4/data/Nordic/nrf51.svd \
         > $td/nrf51.svd

    curl -L \
         https://raw.githubusercontent.com/posborne/cmsis-svd/python-0.4/data/NXP/LPC43xx_svd_v5.svd \
         > $td/LPC43xx_svd_v5.svd

    # test the library
    cargo build $flags
    cargo build $flags --release

    # test the generated code
    svd=STM32F30x.svd
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

    # Test register arrays
    svd=nrf51.svd
    test_gen
    test_gen gpio
    test_gen timer

    # japaric/svd2rust#42
    svd=LPC43xx_svd_v5.svd
    test_gen
    test_gen sct
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
