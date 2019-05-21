set -euxo pipefail

main() {
    cargo rustc --bin svd2rust --target $TARGET --release -- -C lto

    rm -rf stage
    mkdir stage
    cp target/$TARGET/release/svd2rust stage

    pushd stage
    tar czf ../$CRATE_NAME-$TRAVIS_TAG-$TARGET.tar.gz *
    popd

    rm -rf stage
}

main
