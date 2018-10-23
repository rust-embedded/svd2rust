set -euxo pipefail

test_svd() {
    (
        cd $td &&
            curl -LO \
                 https://raw.githubusercontent.com/posborne/cmsis-svd/python-0.4/data/$VENDOR/${1}.svd
    )

    # NOTE we care about errors in svd2rust, but not about errors / warnings in rustfmt
    local cwd=$(pwd)
    pushd $td
    $cwd/target/$TARGET/release/svd2rust -i ${1}.svd

    mv lib.rs src/lib.rs

    # Save off "clean" Cargo toml
    mv $td/Cargo.toml $td/Cargo.toml.bkp

    # include Cargo features into test toml
    cat Cargo.toml.bkp > $td/Cargo.toml
    cat CargoFeatures.toml >> $td/Cargo.toml

    # ignore rustfmt errors
    rustfmt src/lib.rs || true
    popd

    cat $td/Cargo.toml

    cargo check --all-features --manifest-path $td/Cargo.toml

    # Restore pre-feature'd Cargo toml
    mv $td/Cargo.toml.bkp $td/Cargo.toml
}

main() {
    cross check --target $TARGET

    if [ -z ${VENDOR-} ]; then
        return
    fi

    cross build --all-features --target $TARGET --release

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
    echo 'cortex-m = "=0.5.4"' >> $td/Cargo.toml
    echo 'cortex-m-rt = "=0.5.4"' >> $td/Cargo.toml
    echo 'vcell = "0.1.0"' >> $td/Cargo.toml
    echo '[profile.dev]' >> $td/Cargo.toml
    echo 'incremental = false' >> $td/Cargo.toml

    case $VENDOR in
        Atmel)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # BAD-SVD missing resetValue
            # test_svd AT91SAM9CN11
            # test_svd AT91SAM9CN12
            # test_svd AT91SAM9G10
            # test_svd AT91SAM9G15
            # test_svd AT91SAM9G20
            # test_svd AT91SAM9G25
            # test_svd AT91SAM9G35
            # test_svd AT91SAM9M10
            # test_svd AT91SAM9M11
            # test_svd AT91SAM9N12
            # test_svd AT91SAM9X25
            # test_svd AT91SAM9X35
            # test_svd ATSAM3A4C
            # test_svd ATSAM3A8C
            # test_svd ATSAM3N00A
            # test_svd ATSAM3N00B
            # test_svd ATSAM3N0A
            # test_svd ATSAM3N0B
            # test_svd ATSAM3N0C
            # test_svd ATSAM3N1A
            # test_svd ATSAM3N1B
            # test_svd ATSAM3N1C
            # test_svd ATSAM3N2A
            # test_svd ATSAM3N2B
            # test_svd ATSAM3N2C
            # test_svd ATSAM3N4A
            # test_svd ATSAM3N4B
            # test_svd ATSAM3N4C
            # test_svd ATSAM3S1A
            # test_svd ATSAM3S1B
            # test_svd ATSAM3S1C
            # test_svd ATSAM3S2A
            # test_svd ATSAM3S2B
            # test_svd ATSAM3S2C
            # test_svd ATSAM3S4A
            # test_svd ATSAM3S4B
            # test_svd ATSAM3S4C
            # test_svd ATSAM3S8B
            # test_svd ATSAM3S8C
            # test_svd ATSAM3SD8B
            # test_svd ATSAM3SD8C
            # test_svd ATSAM3U1C
            # test_svd ATSAM3U1E
            # test_svd ATSAM3U2C
            # test_svd ATSAM3U2E
            # test_svd ATSAM3U4C
            # test_svd ATSAM3U4E
            # test_svd ATSAM3X4C
            # test_svd ATSAM3X4E
            # test_svd ATSAM3X8C
            # test_svd ATSAM3X8E
            # test_svd ATSAM4S16B
            # test_svd ATSAM4S16C
            # test_svd ATSAM4S8B
            # test_svd ATSAM4S8C
            # test_svd ATSAM4SD32B
            # test_svd ATSAM4SD32C
            # test_svd ATSAMA5D31
            # test_svd ATSAMA5D33
            # test_svd ATSAMA5D34
            # test_svd ATSAMA5D35

            # FIXME(#107) "failed to resolve. Use of undeclared type or module `sercom0`"
            # test_svd ATSAMD21E15A
            # test_svd ATSAMD21E16A
            # test_svd ATSAMD21E17A
            # test_svd ATSAMD21E18A
            # test_svd ATSAMD21G16A
            # test_svd ATSAMD21G17A
            # test_svd ATSAMD21G18A
            # test_svd ATSAMD21J16A
            # test_svd ATSAMD21J17A
            # test_svd ATSAMD21J18A
            # test_svd ATSAMR21E16A
            # test_svd ATSAMR21E17A
            # test_svd ATSAMR21E18A
            # test_svd ATSAMR21G16A
            # test_svd ATSAMR21G17A
            # test_svd ATSAMR21G18A
        ;;

        Freescale)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # BAD-SVD bad enumeratedValue value
            # test_svd MKV56F20
            # test_svd MKV56F22
            # test_svd MKV56F24
            # test_svd MKV58F20
            # test_svd MKV58F22
            # test_svd MKV58F24

            # BAD-SVD field names are equivalent when case is ignored
            # test_svd MK61F15
            # test_svd MK61F15WS
            # test_svd MK70F12
            # test_svd MK70F15
            # test_svd MK70F15WS

            # OK
            # NOTE it would take too long to test all these so we only test one third
            test_svd MK02F12810
            # test_svd MK10D10
            # test_svd MK10D5
            test_svd MK10D7
            # test_svd MK10DZ10
            # test_svd MK10F12
            test_svd MK11D5
            # test_svd MK11D5WS
            # test_svd MK11DA5
            test_svd MK12D5
            # test_svd MK20D10
            # test_svd MK20D5
            test_svd MK20D7
            # test_svd MK20DZ10
            # test_svd MK20F12
            test_svd MK21D5
            # test_svd MK21D5WS
            # test_svd MK21DA5
            test_svd MK21F12
            # test_svd MK21FA12
            # test_svd MK22D5
            test_svd MK22F12
            # test_svd MK22F12810
            # test_svd MK22F25612
            test_svd MK22F51212
            # test_svd MK22FA12
            # test_svd MK24F12
            test_svd MK24F25612
            # test_svd MK26F18
            # test_svd MK30D10
            test_svd MK30D7
            # test_svd MK30DZ10
            # test_svd MK40D10
            test_svd MK40D7
            # test_svd MK40DZ10
            # test_svd MK50D10
            test_svd MK50D7
            # test_svd MK50DZ10
            # test_svd MK51D10
            test_svd MK51D7
            # test_svd MK51DZ10
            # test_svd MK52D10
            test_svd MK52DZ10
            # test_svd MK53D10
            # test_svd MK53DZ10
            test_svd MK60D10
            # test_svd MK60DZ10
            # test_svd MK60F15
            test_svd MK63F12
            # test_svd MK64F12
            # test_svd MK65F18
            test_svd MK66F18
            # test_svd MK80F25615
            # test_svd MK81F25615
            test_svd MK82F25615
            # test_svd MKE14F16
            # test_svd MKE14Z7
            test_svd MKE15Z7
            # test_svd MKE16F16
            # test_svd MKE18F16
            test_svd MKL28T7_CORE0
            # test_svd MKL28T7_CORE1
            # test_svd MKL28Z7
            test_svd MKL81Z7
            # test_svd MKL82Z7
            # test_svd MKS22F12
            test_svd MKV10Z1287
            # test_svd MKV10Z7
            # test_svd MKV11Z7
            test_svd MKV30F12810
            # test_svd MKV31F12810
            # test_svd MKV31F25612
            test_svd MKV31F51212
            # test_svd MKV40F15
            # test_svd MKV42F16
            test_svd MKV43F15
            # test_svd MKV44F15
            # test_svd MKV44F16
            test_svd MKV45F15
            # test_svd MKV46F15
            # test_svd MKV46F16
            test_svd MKW20Z4
            # test_svd MKW21D5
            # test_svd MKW21Z4
            test_svd MKW22D5
            # test_svd MKW24D5
            # test_svd MKW30Z4
            test_svd MKW31Z4
            # test_svd MKW40Z4
            # test_svd MKW41Z4

            # #92 regression tests
            # NOTE it would take too long to test all these so we only test one third
            test_svd MKE02Z4
            # test_svd MKE04Z1284
            # test_svd MKE04Z4
            test_svd MKE06Z4
            # test_svd MKE14D7
            # test_svd MKE15D7
            test_svd MKL02Z4
            # test_svd MKL03Z4
            # test_svd MKL04Z4
            test_svd MKL05Z4
            # test_svd MKL13Z644
            # test_svd MKL14Z4
            test_svd MKL15Z4
            # test_svd MKL16Z4
            # test_svd MKL17Z4
            test_svd MKL17Z644
            # test_svd MKL24Z4
            # test_svd MKL25Z4
            test_svd MKL26Z4
            # test_svd MKL27Z4
            # test_svd MKL27Z644
            test_svd MKL33Z4
            # test_svd MKL33Z644
            # test_svd MKL34Z4
            test_svd MKL36Z4
            # test_svd MKL43Z4
            # test_svd MKL46Z4
            test_svd MKM14ZA5
            # test_svd MKM33ZA5
            # test_svd MKM34Z7
            test_svd MKM34ZA5
            # test_svd MKW01Z4
            # test_svd SKEAZ1284
            test_svd SKEAZN642
            # test_svd SKEAZN84
        ;;

        Fujitsu)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # OK
            test_svd MB9AF10xN
            test_svd MB9AF10xR
            test_svd MB9AF11xK
            test_svd MB9AF11xL
            test_svd MB9AF11xM
            test_svd MB9AF11xN
            test_svd MB9AF12xK
            test_svd MB9AF12xL
            test_svd MB9AF13xK
            test_svd MB9AF13xL
            test_svd MB9AF13xM
            test_svd MB9AF13xN
            test_svd MB9AF14xL
            test_svd MB9AF14xM
            test_svd MB9AF14xN
            test_svd MB9AF15xM
            test_svd MB9AF15xN
            test_svd MB9AF15xR
            test_svd MB9AF1AxL
            test_svd MB9AF1AxM
            test_svd MB9AF1AxN
            test_svd MB9AF31xK
            test_svd MB9AF31xL
            test_svd MB9AF31xM
            test_svd MB9AF31xN
            test_svd MB9AF34xL
            test_svd MB9AF34xM
            test_svd MB9AF34xN
            test_svd MB9AF42xK
            test_svd MB9AF42xL
            test_svd MB9AFA3xL
            test_svd MB9AFA3xM
            test_svd MB9AFA3xN
            test_svd MB9AFA4xL
            test_svd MB9AFA4xM
            test_svd MB9AFA4xN
            test_svd MB9AFAAxL
            test_svd MB9AFAAxM
            test_svd MB9AFAAxN
            test_svd MB9AFB4xL
            test_svd MB9AFB4xM
            test_svd MB9AFB4xN
            test_svd MB9B160L
            test_svd MB9B160R
            test_svd MB9B360L
            test_svd MB9B360R
            test_svd MB9B460L
            test_svd MB9B460R
            test_svd MB9B560L
            test_svd MB9B560R
            test_svd MB9BF10xN
            test_svd MB9BF10xR
            test_svd MB9BF11xN
            test_svd MB9BF11xR
            test_svd MB9BF11xS
            test_svd MB9BF11xT
            test_svd MB9BF12xJ
            test_svd MB9BF12xK
            test_svd MB9BF12xL
            test_svd MB9BF12xM
            test_svd MB9BF12xS
            test_svd MB9BF12xT
            test_svd MB9BF21xS
            test_svd MB9BF21xT
            test_svd MB9BF30xN
            test_svd MB9BF30xR
            test_svd MB9BF31xN
            test_svd MB9BF31xR
            test_svd MB9BF31xS
            test_svd MB9BF31xT
            test_svd MB9BF32xK
            test_svd MB9BF32xL
            test_svd MB9BF32xM
            test_svd MB9BF32xS
            test_svd MB9BF32xT
            test_svd MB9BF40xN
            test_svd MB9BF40xR
            test_svd MB9BF41xN
            test_svd MB9BF41xR
            test_svd MB9BF41xS
            test_svd MB9BF41xT
            test_svd MB9BF42xS
            test_svd MB9BF42xT
            test_svd MB9BF50xN
            test_svd MB9BF50xR
            test_svd MB9BF51xN
            test_svd MB9BF51xR
            test_svd MB9BF51xS
            test_svd MB9BF51xT
            test_svd MB9BF52xK
            test_svd MB9BF52xL
            test_svd MB9BF52xM
            test_svd MB9BF52xS
            test_svd MB9BF52xT
            test_svd MB9BF61xS
            test_svd MB9BF61xT
            test_svd MB9BFD1xS
            test_svd MB9BFD1xT
            test_svd S6E1A1
            test_svd S6E2CC
        ;;

        Holtek)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # OK
            test_svd ht32f125x
            test_svd ht32f175x
            test_svd ht32f275x
        ;;

        # test other targets (architectures)
        OTHER)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.1.0"' >> $td/Cargo.toml

            echo '[dependencies.msp430]' >> $td/Cargo.toml
            echo 'version = "0.1.0"' >> $td/Cargo.toml

            # echo '[dependencies.riscv]' >> $td/Cargo.toml
            # echo 'version = "0.2.0"' >> $td/Cargo.toml

            # echo '[dependencies.riscv-rt]' >> $td/Cargo.toml
            # echo 'version = "0.2.0"' >> $td/Cargo.toml

            (
                cd $td &&
                    curl -LO \
                         https://github.com/pftbest/msp430g2553/raw/v0.1.0/msp430g2553.svd
                # cd $td &&
                #     curl -LO \
                #          https://raw.githubusercontent.com/riscv-rust/e310x/master/e310x.svd
            )

            target/$TARGET/release/svd2rust --target msp430 -i $td/msp430g2553.svd | \
                ( rustfmt 2>/dev/null > $td/src/lib.rs || true )

            cargo check --manifest-path $td/Cargo.toml

            target/$TARGET/release/svd2rust --target none -i $td/msp430g2553.svd | \
                ( rustfmt 2>/dev/null > $td/src/lib.rs || true )

            cargo check --manifest-path $td/Cargo.toml

            # target/$TARGET/release/svd2rust --target riscv -i $td/e310x.svd | \
                # ( rustfmt 2>/dev/null > $td/src/lib.rs || true )

            cargo check --manifest-path $td/Cargo.toml
        ;;

        Nordic)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # BAD-SVD two enumeratedValues have the same value
            # test_svd nrf52

            # OK
            test_svd nrf51
        ;;

        Nuvoton)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # OK
            test_svd M051_Series
            test_svd NUC100_Series
        ;;

        NXP)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # BAD-SVD two enumeratedValues have the same name
            # test_svd LPC11Exx_v5
            # test_svd LPC11Uxx_v7
            # test_svd LPC11xx_v6a
            # test_svd LPC11xx_v6
            # test_svd LPC13Uxx_v1
            # test_svd LPC15xx_v0.7
            # test_svd LPC800_v0.3
            # test_svd LPC11E6x_v0.8
            # test_svd LPC176x5x_v0.2
            # test_svd LPC11Cxx_v9

            # BAD-SVD missing resetValue
            # test_svd LPC178x_7x
            # test_svd LPC178x_7x_v0.8
            # test_svd LPC408x_7x_v0.7
            # test_svd LPC11Axxv0.6


            # BAD-SVD bad identifier: contains a '.'
            # test_svd LPC11D14_svd_v4
            # test_svd LPC13xx_svd_v1

            # BAD-SVD bad identifier: contains a '/'
            # test_svd LPC18xx_svd_v18
            # test_svd LPC43xx_svd_v5

            # BAD-SVD uses the identifier '_' to name a reserved bitfield value
            # test_svd LPC1102_4_v4

            # FIXME(???) "duplicate definitions for `write`"
            # #99 regression test
            # test_svd LPC5410x_v0.4
        ;;

        SiliconLabs)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # #99 regression tests
            test_svd SIM3C1x4_SVD
            test_svd SIM3C1x6_SVD
            test_svd SIM3C1x7_SVD
            test_svd SIM3L1x4_SVD
            test_svd SIM3L1x6_SVD
            test_svd SIM3L1x7_SVD
            test_svd SIM3U1x4_SVD
            test_svd SIM3U1x6_SVD
            test_svd SIM3U1x7_SVD

            # FIXME(???) panicked at "c.text.clone()"
            # test_svd SIM3L1x8_SVD
        ;;

        Spansion)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # OK
            test_svd MB9AF12xK
            test_svd MB9AF12xL
            test_svd MB9AF42xK
            test_svd MB9AF42xL
            test_svd MB9BF12xJ
            test_svd MB9BF12xS
            test_svd MB9BF12xT
            test_svd MB9BF16xx
            test_svd MB9BF32xS
            test_svd MB9BF32xT
            test_svd MB9BF36xx
            test_svd MB9BF42xS
            test_svd MB9BF42xT
            test_svd MB9BF46xx
            test_svd MB9BF52xS
            test_svd MB9BF52xT
            test_svd MB9BF56xx

            # #102 regression tests
            # # NOTE it would take too long to test all these so we only test half
            # test_svd MB9AF10xN
            test_svd MB9AF10xR
            # test_svd MB9AF11xK
            test_svd MB9AF11xL
            # test_svd MB9AF11xM
            test_svd MB9AF11xN
            # test_svd MB9AF13xK
            test_svd MB9AF13xL
            # test_svd MB9AF13xM
            test_svd MB9AF13xN
            # test_svd MB9AF14xL
            test_svd MB9AF14xM
            # test_svd MB9AF14xN
            test_svd MB9AF15xM
            # test_svd MB9AF15xN
            test_svd MB9AF15xR
            # test_svd MB9AF31xK
            test_svd MB9AF31xL
            # test_svd MB9AF31xM
            test_svd MB9AF31xN
            # test_svd MB9AF34xL
            test_svd MB9AF34xM
            # test_svd MB9AF34xN
            test_svd MB9AFA3xL
            # test_svd MB9AFA3xM
            test_svd MB9AFA3xN
            # test_svd MB9AFA4xL
            test_svd MB9AFA4xM
            # test_svd MB9AFA4xN
            test_svd MB9AFB4xL
            # test_svd MB9AFB4xM
            test_svd MB9AFB4xN
            # test_svd MB9BF10xN
            test_svd MB9BF10xR
            # test_svd MB9BF11xN
            test_svd MB9BF11xR
            # test_svd MB9BF11xS
            test_svd MB9BF11xT
            # test_svd MB9BF12xK
            test_svd MB9BF12xL
            # test_svd MB9BF12xM
            test_svd MB9BF21xS
            # test_svd MB9BF21xT
            test_svd MB9BF30xN
            # test_svd MB9BF30xR
            test_svd MB9BF31xN
            # test_svd MB9BF31xR
            test_svd MB9BF31xS
            # test_svd MB9BF31xT
            test_svd MB9BF32xK
            # test_svd MB9BF32xL
            test_svd MB9BF32xM
            # test_svd MB9BF40xN
            test_svd MB9BF40xR
            # test_svd MB9BF41xN
            test_svd MB9BF41xR
            # test_svd MB9BF41xS
            test_svd MB9BF41xT
            # test_svd MB9BF50xN
            test_svd MB9BF50xR
            # test_svd MB9BF51xN
            test_svd MB9BF51xR
            # test_svd MB9BF51xS
            test_svd MB9BF51xT
            # test_svd MB9BF52xK
            test_svd MB9BF52xL
            # test_svd MB9BF52xM
            test_svd MB9BF61xS
            # test_svd MB9BF61xT
            test_svd MB9BFD1xS
            # test_svd MB9BFD1xT
        ;;

        STMicro)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # OK
            test_svd STM32F030
            test_svd STM32F031x
            test_svd STM32F042x
            test_svd STM32F072x
            test_svd STM32F091x
            test_svd STM32F0xx
            test_svd STM32F100xx
            test_svd STM32F101xx
            test_svd STM32F102xx
            test_svd STM32F103xx
            test_svd STM32F105xx
            test_svd STM32F107xx
            test_svd STM32F20x
            test_svd STM32F21x
            test_svd STM32F301x
            test_svd STM32F302x
            test_svd STM32F303xE
            test_svd STM32F303x
            test_svd STM32F30x
            test_svd STM32F334x
            test_svd STM32F37x
            test_svd STM32F401xE
            test_svd STM32F401x
            test_svd STM32F40x
            test_svd STM32F411xx
            test_svd STM32F41x
            test_svd STM32F427x
            test_svd STM32F429x
            test_svd STM32F437x
            test_svd STM32F439x
            test_svd STM32F446x
            test_svd STM32F46_79x
            test_svd STM32L100
            test_svd STM32L15xC
            test_svd STM32L15xxE
            test_svd STM32L15xxxA
            test_svd STM32L1xx
            test_svd STM32L4x6
            test_svd STM32W108

            # FIXME(#91) "field is never used: `register`"
            # test_svd STM32L051x
            # test_svd STM32L052x
            # test_svd STM32L053x
            # test_svd STM32L062x
            # test_svd STM32L063x
        ;;

        Toshiba)
            echo '[dependencies.bare-metal]' >> $td/Cargo.toml
            echo 'version = "0.2.0"' >> $td/Cargo.toml

            # BAD-SVD resetValue is bigger than the register size
            # test_svd M365
            # test_svd M367
            # test_svd M368
            # test_svd M369
            # test_svd M36B

            # OK
            test_svd M061
        ;;
    esac

    rm -rf $td
}

if [ -z ${TRAVIS_TAG-} ]; then
    main
fi
