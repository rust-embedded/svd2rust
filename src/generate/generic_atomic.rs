mod atomic {
    use portable_atomic::Ordering;

    pub trait AtomicOperations {
        unsafe fn atomic_or(ptr: *mut Self, val: Self);
        unsafe fn atomic_and(ptr: *mut Self, val: Self);
        unsafe fn atomic_xor(ptr: *mut Self, val: Self);
    }

    macro_rules! impl_atomics {
        ($U:ty, $Atomic:ty) => {
            impl AtomicOperations for $U {
                unsafe fn atomic_or(ptr: *mut Self, val: Self) {
                    (*(ptr as *const $Atomic)).fetch_or(val, Ordering::SeqCst);
                }

                unsafe fn atomic_and(ptr: *mut Self, val: Self) {
                    (*(ptr as *const $Atomic)).fetch_and(val, Ordering::SeqCst);
                }

                unsafe fn atomic_xor(ptr: *mut Self, val: Self) {
                    (*(ptr as *const $Atomic)).fetch_xor(val, Ordering::SeqCst);
                }
            }
        };
    }

    impl_atomics!(u8, portable_atomic::AtomicU8);
    impl_atomics!(i8, portable_atomic::AtomicI8);
    impl_atomics!(u16, portable_atomic::AtomicU16);
    impl_atomics!(i16, portable_atomic::AtomicI16);

    // Exclude 16-bit archs from 32-bit atomics
    #[cfg(not(target_pointer_width = "16"))]
    impl_atomics!(u32, portable_atomic::AtomicU32);
    #[cfg(not(target_pointer_width = "16"))]
    impl_atomics!(i32, portable_atomic::AtomicI32);
}
use atomic::AtomicOperations;

impl<REG: Readable + Writable> Reg<REG>
where
    REG::Ux: AtomicOperations + Default + core::ops::Not<Output = REG::Ux>,
{
    /// Set high every bit in the register that was set in the write proxy. Leave other bits
    /// untouched. The write is done in a single atomic instruction.
    ///
    /// # Safety
    ///
    /// The resultant bit pattern may not be valid for the register.
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
    ///
    /// # Safety
    ///
    /// The resultant bit pattern may not be valid for the register.
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
    ///
    /// # Safety
    ///
    /// The resultant bit pattern may not be valid for the register.
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
