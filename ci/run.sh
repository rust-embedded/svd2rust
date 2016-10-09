set -ex

gen() {
    cargo run --release -- -i /tmp/STM32F30x.svd $1 > /dev/null
}

main() {
    export LD_LIBRARY_PATH=$(rustc --print sysroot)/lib/rustlib/${1}/lib
    echo $LD_LIBRARY_PATH

    curl -L \
         https://raw.githubusercontent.com/posborne/cmsis-svd/master/data/STMicro/STM32F30x.svd \
         > /tmp/STM32F30x.svd

    cargo build --release

    gen
    gen dbgmcu
    gen gpioa
    gen i2c1
    gen rcc
    gen spi1
    gen tim6
}

main $1
