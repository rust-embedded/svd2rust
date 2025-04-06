/// Trait implemented by registers that can be used to unlock another
/// configuration change protected register.
///
/// A type reference to an unlock register needs to be defined in every
/// implementation of a [`Protected`]
pub trait UnlockRegister {
    /// A raw pointer to the location of the CCP register in data space
    const PTR: *mut u8;
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

        unsafe {
            core::arch::asm!(
                // Write the CCP register with the desired magic
                "ldi {magicreg}, {magic}",
                "out {ccpreg}, {magicreg}",

                // Then immediately write the protected register
                "st X, {perval}",

                magic = const REG::MAGIC,
                ccpreg = const unsafe { core::mem::transmute::<_, i16>(REG::CcpReg::PTR) as i32 },

                in("X") self.register.as_ptr(),
                perval = in(reg) val,

                magicreg = out (reg) _   // mark the magicreg as clobbered
            );
        }
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

        unsafe {
            core::arch::asm!(
                // Write the CCP register with the desired magic
                "ldi {magicreg}, {magic}",
                "out {ccpreg}, {magicreg}",

                // Then immediately write the protected register
                "st X, {perval}",

                magic = const REG::MAGIC,
                ccpreg = const unsafe { core::mem::transmute::<_, i16>(REG::CcpReg::PTR) as i32 },

                in("X") self.register.as_ptr(),
                perval = in(reg) val,

                magicreg = out (reg) _   // mark the magicreg as clobbered
            );
        }
    }
}
