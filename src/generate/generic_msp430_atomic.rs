use msp430_atomic::AtomicOperations;

impl<REG: Readable + Writable> Reg<REG>
where
    REG::Ux: AtomicOperations + Default + core::ops::Not<Output = REG::Ux>,
{
    /// Set high every bit in the register that was set in the write proxy. Leave other bits
    /// untouched. The write is done in a single atomic instruction.
    #[inline(always)]
    pub unsafe fn set_bits<F>(&self, f: F)
    where
        F: FnOnce(&mut REG::Writer) -> &mut W<REG>,
    {
        let bits = f(&mut REG::Writer::from(W {
            bits: Default::default(),
            _reg: marker::PhantomData,
        }))
        .bits;
        REG::Ux::atomic_or(self.register.as_ptr(), bits);
    }

    /// Clear every bit in the register that was cleared in the write proxy. Leave other bits
    /// untouched. The write is done in a single atomic instruction.
    #[inline(always)]
    pub unsafe fn clear_bits<F>(&self, f: F)
    where
        F: FnOnce(&mut REG::Writer) -> &mut W<REG>,
    {
        let bits = f(&mut REG::Writer::from(W {
            bits: !REG::Ux::default(),
            _reg: marker::PhantomData,
        }))
        .bits;
        REG::Ux::atomic_and(self.register.as_ptr(), bits);
    }

    /// Toggle every bit in the register that was set in the write proxy. Leave other bits
    /// untouched. The write is done in a single atomic instruction.
    #[inline(always)]
    pub unsafe fn toggle_bits<F>(&self, f: F)
    where
        F: FnOnce(&mut REG::Writer) -> &mut W<REG>,
    {
        let bits = f(&mut REG::Writer::from(W {
            bits: Default::default(),
            _reg: marker::PhantomData,
        }))
        .bits;
        REG::Ux::atomic_xor(self.register.as_ptr(), bits);
    }
}
