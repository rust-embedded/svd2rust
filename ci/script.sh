set -ex

test_svd() {
    curl -L \
         https://raw.githubusercontent.com/posborne/cmsis-svd/python-0.4/data/$1/${2}.svd \
         > $td/${2}.svd
    target/$TARGET/release/svd2rust -i $td/${2}.svd > $td/src/lib.rs
    cargo build --manifest-path $td/Cargo.toml
}

main() {
    cross build --target $TARGET
    cross build --target $TARGET --release

    if [ ! -z $DISABLE_TESTS ]; then
        return
    fi

    case $TRAVIS_OS_NAME in
        linux)
            td=$(mktemp -d)
            ;;
        osx)
            td=$(mktemp -d -t tmp)
            ;;
    esac

    # test crate
    cargo init --name foo $td
    echo 'cortex-m = "0.2.0"' >> $td/Cargo.toml
    echo 'vcell = "0.1.0"' >> $td/Cargo.toml

    # FIXME
    # test_svd Atmel AT91SAM9CN11

    test_svd Freescale MK81F25615

    test_svd Fujitsu MB9AF10xN

    test_svd Holtek ht32f125x

    test_svd Nordic nrf51

    # FIXME
    # test_svd NXP LPC43xx_svd_v5.svd

    test_svd Nuvoton M051_Series

    test_svd SiliconLabs SIM3C1x4_SVD

    test_svd Spansion MB9AF10xN

    test_svd STMicro STM32F100xx
    test_svd STMicro STM32F103xx
    test_svd STMicro STM32F30x

    # FIXME
    # test_svd TexasInstrument TM4C1230C3PM

    test_svd Toshiba M061

    rm -rf $td
}

if [ -z $TRAVIS_TAG ]; then
    main
fi
