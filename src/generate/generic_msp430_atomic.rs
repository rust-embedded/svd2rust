impl<U, REG> Reg<U, REG>
where
    Self: Readable + Writable,
    U: AtomicOperations + Default + Copy + Not<Output = U>,
{
    ///Set high every bit in the register that was set in the write proxy. Leave other bits
    ///untouched. The write is done in a single atomic instruction.
    #[inline(always)]
    pub unsafe fn set_bits<F>(&self, f: F)
    where
        F: FnOnce(&mut W<U, Self>) -> &mut W<U, Self>,
    {
        let bits = f(&mut W {
            bits: Default::default(),
            _reg: marker::PhantomData,
        })
        .bits;
        U::atomic_or(self.register.as_ptr(), bits);
    }

    ///Clear every bit in the register that was cleared in the write proxy. Leave other bits
    ///untouched. The write is done in a single atomic instruction.
    #[inline(always)]
    pub unsafe fn clear_bits<F>(&self, f: F)
    where
        F: FnOnce(&mut W<U, Self>) -> &mut W<U, Self>,
    {
        let bits = f(&mut W {
            bits: !U::default(),
            _reg: marker::PhantomData,
        })
        .bits;
        U::atomic_and(self.register.as_ptr(), bits);
    }

    ///Toggle every bit in the register that was set in the write proxy. Leave other bits
    ///untouched. The write is done in a single atomic instruction.
    #[inline(always)]
    pub unsafe fn toggle_bits<F>(&self, f: F)
    where
        F: FnOnce(&mut W<U, Self>) -> &mut W<U, Self>,
    {
        let bits = f(&mut W {
            bits: Default::default(),
            _reg: marker::PhantomData,
        })
        .bits;
        U::atomic_xor(self.register.as_ptr(), bits);
    }
}
