use core::marker;
pub use reg::*;

/// Generic peripheral accessor
pub struct Periph<RB, const A: usize> {
    _marker: marker::PhantomData<RB>,
}

unsafe impl<RB, const A: usize> Send for Periph<RB, A> {}

impl<RB, const A: usize> Periph<RB, A> {
    ///Pointer to the register block
    pub const PTR: *const RB = A as *const _;

    ///Return the pointer to the register block
    #[inline(always)]
    pub const fn ptr() -> *const RB {
        Self::PTR
    }

    /// Steal an instance of this peripheral
    ///
    /// # Safety
    ///
    /// Ensure that the new instance of the peripheral cannot be used in a way
    /// that may race with any existing instances, for example by only
    /// accessing read-only or write-only registers, or by consuming the
    /// original peripheral and using critical sections to coordinate
    /// access between multiple new instances.
    ///
    /// Additionally, other software such as HALs may rely on only one
    /// peripheral instance existing to ensure memory safety; ensure
    /// no stolen instances are passed to such software.
    pub unsafe fn steal() -> Self {
        Self {
            _marker: marker::PhantomData,
        }
    }
}

impl<RB, const A: usize> core::ops::Deref for Periph<RB, A> {
    type Target = RB;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { &*Self::PTR }
    }
}

#[doc(hidden)]
pub mod raw {
    use super::{marker, BitM, FieldSpec, FromBits, RegisterSpec, ToBits, Unsafe, Writable};

    pub struct R<REG: RegisterSpec> {
        pub(crate) bits: REG::Ux,
        pub(super) _reg: marker::PhantomData<REG>,
    }

    impl<REG: RegisterSpec> FromBits<REG::Ux> for R<REG> {
        #[inline(always)]
        unsafe fn from_bits(bits: REG::Ux) -> Self {
            Self {
                bits,
                _reg: marker::PhantomData,
            }
        }
    }

    pub struct W<REG: RegisterSpec> {
        ///Writable bits
        pub(crate) bits: REG::Ux,
        pub(super) _reg: marker::PhantomData<REG>,
    }

    impl<REG: RegisterSpec> FromBits<REG::Ux> for W<REG> {
        #[inline(always)]
        unsafe fn from_bits(bits: REG::Ux) -> Self {
            Self {
                bits,
                _reg: marker::PhantomData,
            }
        }
    }

    impl<REG: RegisterSpec> ToBits<REG::Ux> for W<REG> {
        #[inline(always)]
        fn to_bits(&self) -> REG::Ux {
            self.bits
        }
    }

    pub struct FieldReader<FI = u8>
    where
        FI: FieldSpec,
    {
        pub(crate) bits: FI::Ux,
        _reg: marker::PhantomData<FI>,
    }

    impl<FI: FieldSpec> FieldReader<FI> {
        /// Creates a new instance of the reader.
        #[allow(unused)]
        #[inline(always)]
        pub(crate) const fn new(bits: FI::Ux) -> Self {
            Self {
                bits,
                _reg: marker::PhantomData,
            }
        }
    }

    pub struct BitReader<FI = bool> {
        pub(crate) bits: bool,
        _reg: marker::PhantomData<FI>,
    }

    impl<FI> BitReader<FI> {
        /// Creates a new instance of the reader.
        #[allow(unused)]
        #[inline(always)]
        pub(crate) const fn new(bits: bool) -> Self {
            Self {
                bits,
                _reg: marker::PhantomData,
            }
        }
    }

    #[must_use = "after creating `FieldWriter` you need to call field value setting method"]
    pub struct FieldWriter<'a, REG, const WI: u8, FI = u8, Safety = Unsafe>
    where
        REG: Writable + RegisterSpec,
        FI: FieldSpec,
    {
        pub(crate) w: &'a mut W<REG>,
        pub(crate) o: u8,
        _field: marker::PhantomData<(FI, Safety)>,
    }

    impl<'a, REG, const WI: u8, FI, Safety> FieldWriter<'a, REG, WI, FI, Safety>
    where
        REG: Writable + RegisterSpec,
        FI: FieldSpec,
    {
        /// Creates a new instance of the writer
        #[allow(unused)]
        #[inline(always)]
        pub(crate) fn new(w: &'a mut W<REG>, o: u8) -> Self {
            Self {
                w,
                o,
                _field: marker::PhantomData,
            }
        }
    }

    #[must_use = "after creating `BitWriter` you need to call bit setting method"]
    pub struct BitWriter<'a, REG, FI = bool, M = BitM>
    where
        REG: Writable + RegisterSpec,
        bool: From<FI>,
    {
        pub(crate) w: &'a mut W<REG>,
        pub(crate) o: u8,
        _field: marker::PhantomData<(FI, M)>,
    }

    impl<'a, REG, FI, M> BitWriter<'a, REG, FI, M>
    where
        REG: Writable + RegisterSpec,
        bool: From<FI>,
    {
        /// Creates a new instance of the writer
        #[allow(unused)]
        #[inline(always)]
        pub(crate) fn new(w: &'a mut W<REG>, o: u8) -> Self {
            Self {
                w,
                o,
                _field: marker::PhantomData,
            }
        }
    }
}

/// Register reader.
///
/// Result of the `read` methods of registers. Also used as a closure argument in the `modify`
/// method.
pub type R<REG> = raw::R<REG>;

impl<REG: RegisterSpec> R<REG> {
    /// Reads raw bits from register.
    #[inline(always)]
    pub const fn bits(&self) -> REG::Ux {
        self.bits
    }
}

impl<REG: RegisterSpec, FI> PartialEq<FI> for R<REG>
where
    REG::Ux: PartialEq,
    FI: Copy,
    REG::Ux: From<FI>,
{
    #[inline(always)]
    fn eq(&self, other: &FI) -> bool {
        self.bits.eq(&REG::Ux::from(*other))
    }
}

/// Register writer.
///
/// Used as an argument to the closures in the `write` and `modify` methods of the register.
pub type W<REG> = raw::W<REG>;

impl<REG: Writable> W<REG> {
    /// Writes raw bits to the register.
    ///
    /// # Safety
    ///
    /// Passing incorrect value can cause undefined behaviour. See reference manual
    #[inline(always)]
    pub unsafe fn bits(&mut self, bits: REG::Ux) -> &mut Self {
        self.bits = bits;
        self
    }
}
impl<REG> W<REG>
where
    REG: Writable<Safety = Safe>,
{
    /// Writes raw bits to the register.
    #[inline(always)]
    pub fn set(&mut self, bits: REG::Ux) -> &mut Self {
        self.bits = bits;
        self
    }
}

/// Field reader.
///
/// Result of the `read` methods of fields.
pub type FieldReader<FI = u8> = raw::FieldReader<FI>;

/// Bit-wise field reader
pub type BitReader<FI = bool> = raw::BitReader<FI>;

impl<FI: FieldSpec> FieldReader<FI> {
    /// Reads raw bits from field.
    #[inline(always)]
    pub const fn bits(&self) -> FI::Ux {
        self.bits
    }
}

impl<FI: FieldSpec> core::fmt::Debug for FieldReader<FI> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.bits, f)
    }
}

impl<FI> PartialEq<FI> for FieldReader<FI>
where
    FI: FieldSpec + Copy,
{
    #[inline(always)]
    fn eq(&self, other: &FI) -> bool {
        self.bits.eq(&FI::Ux::from(*other))
    }
}

impl<FI> PartialEq<FI> for BitReader<FI>
where
    FI: Copy,
    bool: From<FI>,
{
    #[inline(always)]
    fn eq(&self, other: &FI) -> bool {
        self.bits.eq(&bool::from(*other))
    }
}

impl<FI> BitReader<FI> {
    /// Value of the field as raw bits.
    #[inline(always)]
    pub const fn bit(&self) -> bool {
        self.bits
    }
    /// Returns `true` if the bit is clear (0).
    #[inline(always)]
    pub const fn bit_is_clear(&self) -> bool {
        !self.bit()
    }
    /// Returns `true` if the bit is set (1).
    #[inline(always)]
    pub const fn bit_is_set(&self) -> bool {
        self.bit()
    }
}

impl<FI> core::fmt::Debug for BitReader<FI> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.bits, f)
    }
}

/// Write field Proxy
pub type FieldWriter<'a, REG, const WI: u8, FI = u8, Safety = Unsafe> =
    raw::FieldWriter<'a, REG, WI, FI, Safety>;

impl<REG, const WI: u8, FI, Safety> FieldWriter<'_, REG, WI, FI, Safety>
where
    REG: Writable + RegisterSpec,
    FI: FieldSpec,
{
    /// Field width
    pub const WIDTH: u8 = WI;

    /// Field width
    #[inline(always)]
    pub const fn width(&self) -> u8 {
        WI
    }

    /// Field offset
    #[inline(always)]
    pub const fn offset(&self) -> u8 {
        self.o
    }
}

impl<'a, REG, const WI: u8, FI, Safety> FieldWriter<'a, REG, WI, FI, Safety>
where
    REG: Writable + RegisterSpec,
    FI: FieldSpec,
    REG::Ux: From<FI::Ux>,
{
    /// Writes raw bits to the field
    ///
    /// # Safety
    ///
    /// Passing incorrect value can cause undefined behaviour. See reference manual
    #[inline(always)]
    pub unsafe fn bits(self, value: FI::Ux) -> &'a mut W<REG> {
        self.w.bits &= !(REG::Ux::mask::<WI>() << self.o);
        self.w.bits |= (REG::Ux::from(value) & REG::Ux::mask::<WI>()) << self.o;
        self.w
    }
}

impl<'a, REG, const WI: u8, FI> FieldWriter<'a, REG, WI, FI, Safe>
where
    REG: Writable + RegisterSpec,
    FI: FieldSpec,
    REG::Ux: From<FI::Ux>,
{
    /// Writes raw bits to the field
    #[inline(always)]
    pub fn set(self, value: FI::Ux) -> &'a mut W<REG> {
        unsafe { self.bits(value) }
    }
}

impl<'a, REG, const WI: u8, FI, const MIN: u64, const MAX: u64>
    FieldWriter<'a, REG, WI, FI, Range<MIN, MAX>>
where
    REG: Writable + RegisterSpec,
    FI: FieldSpec,
    REG::Ux: From<FI::Ux>,
    u64: From<FI::Ux>,
{
    /// Writes raw bits to the field
    #[inline(always)]
    pub fn set(self, value: FI::Ux) -> &'a mut W<REG> {
        {
            let value = u64::from(value);
            assert!(value >= MIN && value <= MAX);
        }
        unsafe { self.bits(value) }
    }
}

impl<'a, REG, const WI: u8, FI, const MIN: u64> FieldWriter<'a, REG, WI, FI, RangeFrom<MIN>>
where
    REG: Writable + RegisterSpec,
    FI: FieldSpec,
    REG::Ux: From<FI::Ux>,
    u64: From<FI::Ux>,
{
    /// Writes raw bits to the field
    #[inline(always)]
    pub fn set(self, value: FI::Ux) -> &'a mut W<REG> {
        {
            let value = u64::from(value);
            assert!(value >= MIN);
        }
        unsafe { self.bits(value) }
    }
}

impl<'a, REG, const WI: u8, FI, const MAX: u64> FieldWriter<'a, REG, WI, FI, RangeTo<MAX>>
where
    REG: Writable + RegisterSpec,
    FI: FieldSpec,
    REG::Ux: From<FI::Ux>,
    u64: From<FI::Ux>,
{
    /// Writes raw bits to the field
    #[inline(always)]
    pub fn set(self, value: FI::Ux) -> &'a mut W<REG> {
        {
            let value = u64::from(value);
            assert!(value <= MAX);
        }
        unsafe { self.bits(value) }
    }
}

impl<'a, REG, const WI: u8, FI, Safety> FieldWriter<'a, REG, WI, FI, Safety>
where
    REG: Writable + RegisterSpec,
    FI: IsEnum,
    REG::Ux: From<FI::Ux>,
{
    /// Writes `variant` to the field
    #[inline(always)]
    pub fn variant(self, variant: FI) -> &'a mut W<REG> {
        unsafe { self.bits(FI::Ux::from(variant)) }
    }
}

macro_rules! bit_proxy {
    ($writer:ident, $mwv:ident) => {
        /// Bit-wise write field proxy
        pub type $writer<'a, REG, FI = bool> = raw::BitWriter<'a, REG, FI, $mwv>;

        impl<'a, REG, FI> $writer<'a, REG, FI>
        where
            REG: Writable + RegisterSpec,
            bool: From<FI>,
        {
            /// Field width
            pub const WIDTH: u8 = 1;

            /// Field width
            #[inline(always)]
            pub const fn width(&self) -> u8 {
                Self::WIDTH
            }

            /// Field offset
            #[inline(always)]
            pub const fn offset(&self) -> u8 {
                self.o
            }

            /// Writes bit to the field
            #[inline(always)]
            pub fn bit(self, value: bool) -> &'a mut W<REG> {
                self.w.bits &= !(REG::Ux::ONE << self.o);
                self.w.bits |= (REG::Ux::from(value) & REG::Ux::ONE) << self.o;
                self.w
            }
            /// Writes `variant` to the field
            #[inline(always)]
            pub fn variant(self, variant: FI) -> &'a mut W<REG> {
                self.bit(bool::from(variant))
            }
        }
    };
}

bit_proxy!(BitWriter, BitM);
bit_proxy!(BitWriter1S, Bit1S);
bit_proxy!(BitWriter0C, Bit0C);
bit_proxy!(BitWriter1C, Bit1C);
bit_proxy!(BitWriter0S, Bit0S);
bit_proxy!(BitWriter1T, Bit1T);
bit_proxy!(BitWriter0T, Bit0T);

impl<'a, REG, FI> BitWriter<'a, REG, FI>
where
    REG: Writable + RegisterSpec,
    bool: From<FI>,
{
    /// Sets the field bit
    #[inline(always)]
    pub fn set_bit(self) -> &'a mut W<REG> {
        self.w.bits |= REG::Ux::ONE << self.o;
        self.w
    }
    /// Clears the field bit
    #[inline(always)]
    pub fn clear_bit(self) -> &'a mut W<REG> {
        self.w.bits &= !(REG::Ux::ONE << self.o);
        self.w
    }
}

impl<'a, REG, FI> BitWriter1S<'a, REG, FI>
where
    REG: Writable + RegisterSpec,
    bool: From<FI>,
{
    /// Sets the field bit
    #[inline(always)]
    pub fn set_bit(self) -> &'a mut W<REG> {
        self.w.bits |= REG::Ux::ONE << self.o;
        self.w
    }
}

impl<'a, REG, FI> BitWriter0C<'a, REG, FI>
where
    REG: Writable + RegisterSpec,
    bool: From<FI>,
{
    /// Clears the field bit
    #[inline(always)]
    pub fn clear_bit(self) -> &'a mut W<REG> {
        self.w.bits &= !(REG::Ux::ONE << self.o);
        self.w
    }
}

impl<'a, REG, FI> BitWriter1C<'a, REG, FI>
where
    REG: Writable + RegisterSpec,
    bool: From<FI>,
{
    ///Clears the field bit by passing one
    #[inline(always)]
    pub fn clear_bit_by_one(self) -> &'a mut W<REG> {
        self.w.bits |= REG::Ux::ONE << self.o;
        self.w
    }
}

impl<'a, REG, FI> BitWriter0S<'a, REG, FI>
where
    REG: Writable + RegisterSpec,
    bool: From<FI>,
{
    ///Sets the field bit by passing zero
    #[inline(always)]
    pub fn set_bit_by_zero(self) -> &'a mut W<REG> {
        self.w.bits &= !(REG::Ux::ONE << self.o);
        self.w
    }
}

impl<'a, REG, FI> BitWriter1T<'a, REG, FI>
where
    REG: Writable + RegisterSpec,
    bool: From<FI>,
{
    ///Toggle the field bit by passing one
    #[inline(always)]
    pub fn toggle_bit(self) -> &'a mut W<REG> {
        self.w.bits |= REG::Ux::ONE << self.o;
        self.w
    }
}

impl<'a, REG, FI> BitWriter0T<'a, REG, FI>
where
    REG: Writable + RegisterSpec,
    bool: From<FI>,
{
    ///Toggle the field bit by passing zero
    #[inline(always)]
    pub fn toggle_bit(self) -> &'a mut W<REG> {
        self.w.bits &= !(REG::Ux::ONE << self.o);
        self.w
    }
}
