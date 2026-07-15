/// Trait implemented by registers that can be used to unlock another
/// configuration change protected register.
///
/// A type reference to an unlock register needs to be defined in every
/// implementation of a [`Protected`]
pub trait UnlockRegister {
    /// The I/O-space address of the CCP register.
    ///
    /// On every CCP-bearing AVR core (the xmega family as well as the
    /// reduced-core tinys) data-space addresses below 0x40 map one-to-one to
    /// I/O-space addresses, and the code generator rejects unlock registers
    /// at or above 0x40, so this address is usable with the `out` instruction
    /// directly.
    const ADDR: u8;
}

/// Trait to mark a register as configuration change protected on xmega based
/// AVR cores.
///
/// To write into a configuration change protected register, the CPU first has
/// to issue a write a defined magic value to a register called `CCP` in the
/// `CPU` block of the core. After this write access has been performed, the
/// protected register has to be written within the next four instruction for
/// the write to take effect.
pub trait Protected {
    /// The CCP [`UnlockRegister`] that needs to be written with the
    /// [`Self::MAGIC`] value to unlock writes to the protected register.
    type CcpReg: UnlockRegister;

    /// The magic value that needs to be written into the configuration change
    /// protection register [`Self::CcpReg`] to unlock writes to the protected
    /// register
    const MAGIC: u8;
}

/// Performs the CCP unlock sequence and writes `val` to the protected
/// register at `ptr`.
///
/// The unlock window opens with the write to CCP and only spans the next four
/// instructions, so `out` and the protected store must sit in one asm block.
/// The magic value is passed as a register operand instead of being loaded
/// with an `ldi` inside the block: the load happens before the window opens,
/// so the compiler is free to hoist it out of loops or reuse a register that
/// already holds the value (this mirrors avr-libc's `ccp_write_io`, which
/// takes the magic through a `"d"`-constrained operand). `reg_upper` is the
/// `ldi`-capable r16..=r31 class the compiler needs to materialize the
/// constant.
///
/// The protected register is written through the X pointer (`st X`) rather
/// than a compile-time `sts`, because the register address is taken from the
/// register reference at runtime; that keeps the write correct even for
/// `derivedFrom` peripherals, which share one set of register spec types
/// between instances at different base addresses. This sequence is also
/// identical to avr-libc's runtime-address variant and works unchanged on
/// both the xmega family and the reduced-core tinys (whose 7-bit `sts` and
/// 16-register file rule out the other encodings avr-libc picks for
/// compile-time-constant addresses).
///
/// # Safety
///
/// `ptr` must point to the memory-mapped register described by `REG`.
#[inline(always)]
unsafe fn ccp_protected_write<REG>(ptr: *mut u8, val: u8)
where
    REG: RegisterSpec<Ux = u8> + Writable + Protected,
{
    core::arch::asm!(
        // Write the CCP register with the magic to open the unlock window
        "out {ccpreg}, {magic}",

        // Then immediately write the protected register
        "st X, {perval}",

        ccpreg = const REG::CcpReg::ADDR,
        magic = in(reg_upper) REG::MAGIC,

        in("X") ptr,
        perval = in(reg) val,
    );
}

/// Trait implemented by [`Writable`] and [`Protected`] registers which
/// allows writing to the protected register by first writing a magic value to
/// the CCP register.
pub trait ProtectedWritable<REG>
where
    REG: Writable + Protected
{
    /// Write to a CCP protected register by unlocking it first.
    ///
    /// Refer to [`Reg::write`] for usage.
    fn write_protected<F>(&self, f: F)
    where
        F: FnOnce(&mut W<REG>) -> &mut W<REG>;
}

impl<REG> ProtectedWritable<REG> for Reg<REG>
where
    REG: RegisterSpec<Ux = u8> + Writable + Resettable + Protected
{
    /// Unlocks and then writes bits to a `Writable` register.
    ///
    /// Refer to [`Reg::write`] for usage.
    #[inline(always)]
    fn write_protected<F>(&self, f: F)
    where
        F: FnOnce(&mut W<REG>) -> &mut W<REG>
    {
        let val = f(&mut W::<REG>::from(W {
            bits: REG::RESET_VALUE & !REG::ONE_TO_MODIFY_FIELDS_BITMAP
                | REG::ZERO_TO_MODIFY_FIELDS_BITMAP,
            _reg: marker::PhantomData,
        })).bits;

        unsafe { ccp_protected_write::<REG>(self.register.as_ptr(), val) }
    }
}

impl<REG: RegisterSpec<Ux = u8> + Readable + Writable + Protected> Reg<REG> {
    /// Modifies the contents of a protected register by reading and then
    /// unlocking and writing it.
    ///
    /// Refer to [`Reg::modify`] for usage.
    #[inline(always)]
    pub fn modify_protected<F>(&self, f: F)
    where
        for<'w> F: FnOnce(&R<REG>, &'w mut W<REG>) -> &'w mut W<REG>,
    {
        let bits = self.register.get();
        let val = f(
            &R::<REG>::from(R {
                bits,
                _reg: marker::PhantomData,
            }),
            &mut W::<REG>::from(W {
                bits: bits & !REG::ONE_TO_MODIFY_FIELDS_BITMAP
                    | REG::ZERO_TO_MODIFY_FIELDS_BITMAP,
                _reg: marker::PhantomData,
            }),
        )
        .bits;

        unsafe { ccp_protected_write::<REG>(self.register.as_ptr(), val) }
    }
}
