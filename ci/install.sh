set -ex

main() {
    local sort=
    if [ $TRAVIS_OS_NAME = linux ]; then
        sort=sort
    else
        sort=gsort
    fi

    local tag=$(git ls-remote --tags --refs --exit-code https://github.com/japaric/cross \
                    | cut -d/ -f3 \
                    | grep -E '^v[0-9.]+$' \
                    | $sort --version-sort \
                    | tail -n1)
    curl -LSfs https://japaric.github.io/trust/install.sh | \
        sh -s -- \
           --force \
           --git japaric/cross \
           --tag $tag

    if [ ! -z $VENDOR ]; then
        curl -LSfs https://japaric.github.io/trust/install.sh | \
            sh -s -- \
               --force \
               --git japaric/rustfmt-bin \
               --tag v0.8.4-20170605
    fi
}

main
