mod bitbanding {
    use super::*;
    const PERI_ADDRESS_START: usize = 0x4000_0000;
    const PERI_BIT_BAND_BASE: usize = 0x4200_0000;

    impl<REG: Writable> Reg<REG> {
        pub unsafe fn bb_write<F>(&self, f: F, set: bool)
        where
            F: FnOnce(&mut W<REG>) -> u8,
        {
            let addr = self.as_ptr() as usize;
    
            let bit = f(&mut W {
                bits: REG::Ux::default(),
                _reg: marker::PhantomData,
            }) as usize;
    
            let bb_addr = (PERI_BIT_BAND_BASE + (addr - PERI_ADDRESS_START) * 32) + 4 * bit;
            core::ptr::write_volatile(bb_addr as *mut u32, u32::from(set));
        }
        #[inline(always)]
        pub unsafe fn bb_set<F>(&self, f: F)
        where
            F: FnOnce(&mut W<REG>) -> u8,
        {
            self.bb_write(f, true)
        }
        #[inline(always)]
        pub unsafe fn bb_clear<F>(&self, f: F)
        where
            F: FnOnce(&mut W<REG>) -> u8,
        {
            self.bb_write(f, false)
        }
    }
}
