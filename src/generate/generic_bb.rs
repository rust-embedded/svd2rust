impl<REG: Writable> Reg<REG> {
    /// Writes bit with "bit-banding".
    #[inline(always)]
    pub fn bb_write<F, FI>(&self, f: F, set: bool)
    where
        F: FnOnce(&mut W<REG>) -> BitWriter<REG, FI>,
        bool: From<FI>,
    {
        self.bbwrite(f, set)
    }
    #[inline(always)]
    fn bbwrite<F, FI, B>(&self, f: F, set: bool)
    where
        F: FnOnce(&mut W<REG>) -> raw::BitWriter<REG, FI, B>,
        bool: From<FI>,
    {
        // Start address of the peripheral memory region capable of being addressed by bit-banding
        const PERI_ADDRESS_START: usize = 0x4000_0000;
        const PERI_BIT_BAND_BASE: usize = 0x4200_0000;

        let addr = self.as_ptr() as usize;
        let bit = f(&mut W {
            bits: REG::Ux::default(),
            _reg: marker::PhantomData,
        })
        .o as usize;
        let bit = bit as usize;
        let bb_addr = (PERI_BIT_BAND_BASE + (addr - PERI_ADDRESS_START) * 32) + 4 * bit;
        unsafe { core::ptr::write_volatile(bb_addr as *mut u32, u32::from(set)) };
    }

    /// Sets bit with "bit-banding".
    #[inline(always)]
    pub fn bb_set<F, FI, B>(&self, f: F)
    where
        F: FnOnce(&mut W<REG>) -> raw::BitWriter<REG, FI, B>,
        bool: From<FI>,
        B: BBSet,
    {
        self.bbwrite(f, true);
    }

    /// Clears bit with "bit-banding".
    #[inline(always)]
    pub fn bb_clear<F, FI, B>(&self, f: F)
    where
        F: FnOnce(&mut W<REG>) -> raw::BitWriter<REG, FI, B>,
        bool: From<FI>,
        B: BBClear,
    {
        self.bbwrite(f, false);
    }
}

pub trait BBSet {}
pub trait BBClear {}
impl BBSet for BitM {}
impl BBSet for Bit1S {}
impl BBSet for Bit1C {}
impl BBSet for Bit1T {}
impl BBClear for BitM {}
impl BBClear for Bit0S {}
impl BBClear for Bit0C {}
impl BBClear for Bit0T {}
