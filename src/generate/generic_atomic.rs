mod atomic {
    use super::*;
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
                    (*(ptr as *const $Atomic)).or(val, Ordering::SeqCst);
                }

                unsafe fn atomic_and(ptr: *mut Self, val: Self) {
                    (*(ptr as *const $Atomic)).and(val, Ordering::SeqCst);
                }

                unsafe fn atomic_xor(ptr: *mut Self, val: Self) {
                    (*(ptr as *const $Atomic)).xor(val, Ordering::SeqCst);
                }
            }
        };
    }

    impl_atomics!(u8, portable_atomic::AtomicU8);
    impl_atomics!(u16, portable_atomic::AtomicU16);

    // Exclude 16-bit archs from 32-bit atomics
    #[cfg(not(target_pointer_width = "16"))]
    impl_atomics!(u32, portable_atomic::AtomicU32);

    // Enable 64-bit atomics for 64-bit RISCV
    #[cfg(any(target_pointer_width = "64", target_has_atomic = "64"))]
    impl_atomics!(u64, portable_atomic::AtomicU64);

    impl<REG: Readable + Writable> Reg<REG>
    where
        REG::Ux: AtomicOperations,
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
            F: FnOnce(&mut REG::Writer) -> &mut REG::Writer,
        {
            let bits = f(&mut REG::Writer::from_bits(REG::Ux::ZERO)).to_bits();
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
            F: FnOnce(&mut REG::Writer) -> &mut REG::Writer,
        {
            let bits = f(&mut REG::Writer::from_bits(!REG::Ux::ZERO)).to_bits();
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
            F: FnOnce(&mut REG::Writer) -> &mut REG::Writer,
        {
            let bits = f(&mut REG::Writer::from_bits(REG::Ux::ZERO)).to_bits();
            REG::Ux::atomic_xor(self.register.as_ptr(), bits);
        }
    }
}
