set -euxo pipefail

main() {
    if [ $TARGET = x86_64-pc-windows-msvc ]; then
        cargo=cargo
    else
        cargo=cross
    fi

    $cargo rustc --bin svd2rust --target $TARGET --release -- -C lto

    rm -rf stage
    mkdir stage
    cp target/$TARGET/release/svd2rust stage

    pushd stage
    tar czf ../$CRATE_NAME-$TRAVIS_TAG-$TARGET.tar.gz *
    popd

    rm -rf stage
}

main
