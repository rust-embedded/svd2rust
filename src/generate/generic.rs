use core::marker;

/// Raw register type
pub trait RegisterSpec {
    /// Raw register type (`u8`, `u16`, `u32`, ...).
    type Ux: Copy + Default + core::ops::BitOr<Output=Self::Ux> + core::ops::BitAnd<Output=Self::Ux> + core::ops::Not<Output=Self::Ux>;
}

/// Trait implemented by readable registers to enable the `read` method.
///
/// Registers marked with `Writable` can be also `modify`'ed.
pub trait Readable: RegisterSpec {
    /// Result from a call to `read` and argument to `modify`.
    type Reader: From<R<Self>> + core::ops::Deref<Target = R<Self>>;
}

/// Trait implemented by writeable registers.
///
/// This enables the  `write`, `write_with_zero` and `reset` methods.
///
/// Registers marked with `Readable` can be also `modify`'ed.
pub trait Writable: RegisterSpec {
    /// Writer type argument to `write`, et al.
    type Writer: From<W<Self>> + core::ops::DerefMut<Target = W<Self>>;

    /// Specifies the register bits that are not changed if you pass `1` and are changed if you pass `0`
    const ZERO_TO_MODIFY_FIELDS_BITMAP: Self::Ux;

    /// Specifies the register bits that are not changed if you pass `0` and are changed if you pass `1`
    const ONE_TO_MODIFY_FIELDS_BITMAP: Self::Ux;
}

/// Reset value of the register.
///
/// This value is the initial value for the `write` method. It can also be directly written to the
/// register by using the `reset` method.
pub trait Resettable: RegisterSpec {
    /// Reset value of the register.
    const RESET_VALUE: Self::Ux;

    /// Reset value of the register.
    #[inline(always)]
    fn reset_value() -> Self::Ux {
        Self::RESET_VALUE
    }
}

/// This structure provides volatile access to registers.
#[repr(transparent)]
pub struct Reg<REG: RegisterSpec> {
    register: vcell::VolatileCell<REG::Ux>,
    _marker: marker::PhantomData<REG>,
}

unsafe impl<REG: RegisterSpec> Send for Reg<REG> where REG::Ux: Send {}

impl<REG: RegisterSpec> Reg<REG> {
    /// Returns the underlying memory address of register.
    ///
    /// ```ignore
    /// let reg_ptr = periph.reg.as_ptr();
    /// ```
    #[inline(always)]
    pub fn as_ptr(&self) -> *mut REG::Ux {
        self.register.as_ptr()
    }
}

impl<REG: Readable> Reg<REG> {
    /// Reads the contents of a `Readable` register.
    ///
    /// You can read the raw contents of a register by using `bits`:
    /// ```ignore
    /// let bits = periph.reg.read().bits();
    /// ```
    /// or get the content of a particular field of a register:
    /// ```ignore
    /// let reader = periph.reg.read();
    /// let bits = reader.field1().bits();
    /// let flag = reader.field2().bit_is_set();
    /// ```
    #[inline(always)]
    pub fn read(&self) -> REG::Reader {
        REG::Reader::from(R {
            bits: self.register.get(),
            _reg: marker::PhantomData,
        })
    }
}

impl<REG: Resettable + Writable> Reg<REG> {
    /// Writes the reset value to `Writable` register.
    ///
    /// Resets the register to its initial state.
    #[inline(always)]
    pub fn reset(&self) {
        self.register.set(REG::RESET_VALUE)
    }

    /// Writes bits to a `Writable` register.
    ///
    /// You can write raw bits into a register:
    /// ```ignore
    /// periph.reg.write(|w| unsafe { w.bits(rawbits) });
    /// ```
    /// or write only the fields you need:
    /// ```ignore
    /// periph.reg.write(|w| w
    ///     .field1().bits(newfield1bits)
    ///     .field2().set_bit()
    ///     .field3().variant(VARIANT)
    /// );
    /// ```
    /// or an alternative way of saying the same:
    /// ```ignore
    /// periph.reg.write(|w| {
    ///     w.field1().bits(newfield1bits);
    ///     w.field2().set_bit();
    ///     w.field3().variant(VARIANT)
    /// });
    /// ```
    /// In the latter case, other fields will be set to their reset value.
    #[inline(always)]
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut REG::Writer) -> &mut W<REG>
    {
        self.register.set(
            f(&mut REG::Writer::from(W {
                bits: REG::RESET_VALUE & !REG::ONE_TO_MODIFY_FIELDS_BITMAP | REG::ZERO_TO_MODIFY_FIELDS_BITMAP,
                _reg: marker::PhantomData,
            }))
            .bits,
        );
    }
}

impl<REG: Writable> Reg<REG> {
    /// Writes 0 to a `Writable` register.
    ///
    /// Similar to `write`, but unused bits will contain 0.
    ///
    /// # Safety
    ///
    /// Unsafe to use with registers which don't allow to write 0.
    #[inline(always)]
    pub unsafe fn write_with_zero<F>(&self, f: F)
    where
        F: FnOnce(&mut REG::Writer) -> &mut W<REG>
    {
        self.register.set(
            f(&mut REG::Writer::from(W {
                bits: REG::Ux::default(),
                _reg: marker::PhantomData,
            }))
            .bits,
        );
    }
}

impl<REG: Readable + Writable> Reg<REG> {
    /// Modifies the contents of the register by reading and then writing it.
    ///
    /// E.g. to do a read-modify-write sequence to change parts of a register:
    /// ```ignore
    /// periph.reg.modify(|r, w| unsafe { w.bits(
    ///    r.bits() | 3
    /// ) });
    /// ```
    /// or
    /// ```ignore
    /// periph.reg.modify(|_, w| w
    ///     .field1().bits(newfield1bits)
    ///     .field2().set_bit()
    ///     .field3().variant(VARIANT)
    /// );
    /// ```
    /// or an alternative way of saying the same:
    /// ```ignore
    /// periph.reg.modify(|_, w| {
    ///     w.field1().bits(newfield1bits);
    ///     w.field2().set_bit();
    ///     w.field3().variant(VARIANT)
    /// });
    /// ```
    /// Other fields will have the value they had before the call to `modify`.
    #[inline(always)]
    pub fn modify<F>(&self, f: F)
    where
        for<'w> F: FnOnce(&REG::Reader, &'w mut REG::Writer) -> &'w mut W<REG>
    {
        let bits = self.register.get();
        self.register.set(
            f(
                &REG::Reader::from(R {
                    bits,
                    _reg: marker::PhantomData,
                }),
                &mut REG::Writer::from(W {
                    bits: bits & !REG::ONE_TO_MODIFY_FIELDS_BITMAP | REG::ZERO_TO_MODIFY_FIELDS_BITMAP,
                    _reg: marker::PhantomData,
                }),
            )
            .bits,
        );
    }
}

/// Register reader.
///
/// Result of the `read` methods of registers. Also used as a closure argument in the `modify`
/// method.
pub struct R<REG: RegisterSpec + ?Sized> {
    pub(crate) bits: REG::Ux,
    _reg: marker::PhantomData<REG>,
}

impl<REG: RegisterSpec> R<REG> {
    /// Reads raw bits from register.
    #[inline(always)]
    pub fn bits(&self) -> REG::Ux {
        self.bits
    }
}

impl<REG: RegisterSpec, FI> PartialEq<FI> for R<REG>
where
    REG::Ux: PartialEq,
    FI: Copy + Into<REG::Ux>,
{
    #[inline(always)]
    fn eq(&self, other: &FI) -> bool {
        self.bits.eq(&(*other).into())
    }
}

/// Register writer.
///
/// Used as an argument to the closures in the `write` and `modify` methods of the register.
pub struct W<REG: RegisterSpec + ?Sized> {
    ///Writable bits
    pub(crate) bits: REG::Ux,
    _reg: marker::PhantomData<REG>,
}

impl<REG: RegisterSpec> W<REG> {
    /// Writes raw bits to the register.
    ///
    /// # Safety
    ///
    /// Read datasheet or reference manual to find what values are allowed to pass.
    #[inline(always)]
    pub unsafe fn bits(&mut self, bits: REG::Ux) -> &mut Self {
        self.bits = bits;
        self
    }
}

#[doc(hidden)]
pub struct FieldReaderRaw<U, T> {
    pub(crate) bits: U,
    _reg: marker::PhantomData<T>,
}

impl<U, FI> FieldReaderRaw<U, FI>
where
    U: Copy,
{
    /// Creates a new instance of the reader.
    #[allow(unused)]
    #[inline(always)]
    pub(crate) fn new(bits: U) -> Self {
        Self {
            bits,
            _reg: marker::PhantomData,
        }
    }
}

#[doc(hidden)]
pub struct BitReaderRaw<T> {
    pub(crate) bits: bool,
    _reg: marker::PhantomData<T>,
}

impl<FI> BitReaderRaw<FI> {
    /// Creates a new instance of the reader.
    #[allow(unused)]
    #[inline(always)]
    pub(crate) fn new(bits: bool) -> Self {
        Self {
            bits,
            _reg: marker::PhantomData,
        }
    }
}

/// Field reader.
///
/// Result of the `read` methods of fields.
pub type FieldReader<U, FI> = FieldReaderRaw<U, FI>;

/// Bit-wise field reader
pub type BitReader<FI> = BitReaderRaw<FI>;

impl<U, FI> FieldReader<U, FI>
where
    U: Copy,
{
    /// Reads raw bits from field.
    #[inline(always)]
    pub fn bits(&self) -> U {
        self.bits
    }
}

impl<U, FI> PartialEq<FI> for FieldReader<U, FI>
where
    U: PartialEq,
    FI: Copy + Into<U>,
{
    #[inline(always)]
    fn eq(&self, other: &FI) -> bool {
        self.bits.eq(&(*other).into())
    }
}

impl<FI> PartialEq<FI> for BitReader<FI>
where
    FI: Copy + Into<bool>,
{
    #[inline(always)]
    fn eq(&self, other: &FI) -> bool {
        self.bits.eq(&(*other).into())
    }
}

impl<FI> BitReader<FI> {
    /// Value of the field as raw bits.
    #[inline(always)]
    pub fn bit(&self) -> bool {
        self.bits
    }
    /// Returns `true` if the bit is clear (0).
    #[inline(always)]
    pub fn bit_is_clear(&self) -> bool {
        !self.bit()
    }
    /// Returns `true` if the bit is set (1).
    #[inline(always)]
    pub fn bit_is_set(&self) -> bool {
        self.bit()
    }
}

#[doc(hidden)]
pub struct Safe;
#[doc(hidden)]
pub struct Unsafe;

#[doc(hidden)]
pub struct FieldWriterRaw<'a, U, REG, N, FI, Safety, const WI: u8, const O: u8>
where
    REG: Writable + RegisterSpec<Ux = U>,
    FI: Into<N>,
{
    pub(crate) w: &'a mut REG::Writer,
    _field: marker::PhantomData<(N, FI, Safety)>,
}

impl<'a, U, REG, N, FI, Safety, const WI: u8, const O: u8> FieldWriterRaw<'a, U, REG, N, FI, Safety, WI, O>
where
    REG: Writable + RegisterSpec<Ux = U>,
    FI: Into<N>,
{
    /// Creates a new instance of the writer
    #[allow(unused)]
    #[inline(always)]
    pub(crate) fn new(w: &'a mut REG::Writer) -> Self {
        Self {
            w,
            _field: marker::PhantomData,
        }
    }
}

#[doc(hidden)]
pub struct BitWriterRaw<'a, U, REG, FI, M, const O: u8>
where
    REG: Writable + RegisterSpec<Ux = U>,
    FI: Into<bool>,
{
    pub(crate) w: &'a mut REG::Writer,
    _field: marker::PhantomData<(FI, M)>,
}

impl<'a, U, REG, FI, M, const O: u8> BitWriterRaw<'a, U, REG, FI, M, O>
where
    REG: Writable + RegisterSpec<Ux = U>,
    FI: Into<bool>,
{
    /// Creates a new instance of the writer
    #[allow(unused)]
    #[inline(always)]
    pub(crate) fn new(w: &'a mut REG::Writer) -> Self {
        Self {
            w,
            _field: marker::PhantomData,
        }
    }
}

/// Write field Proxy with unsafe `bits`
pub type FieldWriter<'a, U, REG, N, FI, const WI: u8, const O: u8> = FieldWriterRaw<'a, U, REG, N, FI, Unsafe, WI, O>;
/// Write field Proxy with safe `bits`
pub type FieldWriterSafe<'a, U, REG, N, FI, const WI: u8, const O: u8> = FieldWriterRaw<'a, U, REG, N, FI, Safe, WI, O>;


impl<'a, U, REG, N, FI, const WI: u8, const OF: u8> FieldWriter<'a, U, REG, N, FI, WI, OF>
where
    REG: Writable + RegisterSpec<Ux = U>,
    FI: Into<N>,
{
    /// Field width
    pub const WIDTH: u8 = WI;
    /// Field offset
    pub const OFFSET: u8 = OF;
}

impl<'a, U, REG, N, FI, const WI: u8, const OF: u8> FieldWriterSafe<'a, U, REG, N, FI, WI, OF>
where
    REG: Writable + RegisterSpec<Ux = U>,
    FI: Into<N>,
{
    /// Field width
    pub const WIDTH: u8 = WI;
    /// Field offset
    pub const OFFSET: u8 = OF;
}

macro_rules! bit_proxy {
    ($writer:ident, $mwv:ident) => {
        #[doc(hidden)]
        pub struct $mwv;

        /// Bit-wise write field proxy
        pub type $writer<'a, U, REG, FI, const O: u8> = BitWriterRaw<'a, U, REG, FI, $mwv, O>;

        impl<'a, U, REG, FI, const OF: u8> $writer<'a, U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = U>,
            FI: Into<bool>,
        {
            /// Field width
            pub const WIDTH: u8 = 1;
            /// Field offset
            pub const OFFSET: u8 = OF;
        }
    }
}

macro_rules! impl_bit_proxy {
    ($writer:ident, $U:ty) => {
        impl<'a, REG, FI, const OF: u8> $writer<'a, $U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            FI: Into<bool>,
        {
            /// Writes bit to the field
            #[inline(always)]
            pub fn bit(self, value: bool) -> &'a mut REG::Writer {
                self.w.bits = (self.w.bits & !(1 << { OF })) | ((<$U>::from(value) & 1) << { OF });
                self.w
            }
            /// Writes `variant` to the field
            #[inline(always)]
            pub fn variant(self, variant: FI) -> &'a mut REG::Writer {
                self.bit(variant.into())
            }
        }
    }
}

bit_proxy!(BitWriter, BitM);
bit_proxy!(BitWriter1S, Bit1S);
bit_proxy!(BitWriter0C, Bit0C);
bit_proxy!(BitWriter1C, Bit1C);
bit_proxy!(BitWriter0S, Bit0S);
bit_proxy!(BitWriter1T, Bit1T);
bit_proxy!(BitWriter0T, Bit0T);

macro_rules! impl_proxy {
    ($U:ty) => {
        impl<'a, REG, N, FI, const WI: u8, const OF: u8> FieldWriter<'a, $U, REG, N, FI, WI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            N: Into<$U>,
            FI: Into<N>,
        {
            const MASK: $U = <$U>::MAX >> (<$U>::MAX.leading_ones() as u8 - { WI });
            /// Writes raw bits to the field
            ///
            /// # Safety
            ///
            /// Passing incorrect value can cause undefined behaviour. See reference manual
            #[inline(always)]
            pub unsafe fn bits(self, value: N) -> &'a mut REG::Writer {
                self.w.bits =
                    (self.w.bits & !(Self::MASK << { OF })) | ((value.into() & Self::MASK) << { OF });
                self.w
            }
            /// Writes `variant` to the field
            #[inline(always)]
            pub fn variant(self, variant: FI) -> &'a mut REG::Writer {
                unsafe { self.bits(variant.into()) }
            }
        }
        impl<'a, REG, N, FI, const WI: u8, const OF: u8> FieldWriterSafe<'a, $U, REG, N, FI, WI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            N: Into<$U>,
            FI: Into<N>,
        {
            const MASK: $U = <$U>::MAX >> (<$U>::MAX.leading_ones() as u8 - { WI });
            /// Writes raw bits to the field
            #[inline(always)]
            pub fn bits(self, value: N) -> &'a mut REG::Writer {
                self.w.bits =
                    (self.w.bits & !(Self::MASK << { OF })) | ((value.into() & Self::MASK) << { OF });
                self.w
            }
            /// Writes `variant` to the field
            #[inline(always)]
            pub fn variant(self, variant: FI) -> &'a mut REG::Writer {
                self.bits(variant.into())
            }
        }
        impl_bit_proxy!(BitWriter, $U);
        impl_bit_proxy!(BitWriter1S, $U);
        impl_bit_proxy!(BitWriter0C, $U);
        impl_bit_proxy!(BitWriter1C, $U);
        impl_bit_proxy!(BitWriter0S, $U);
        impl_bit_proxy!(BitWriter1T, $U);
        impl_bit_proxy!(BitWriter0T, $U);
        impl<'a, REG, FI, const OF: u8> BitWriter<'a, $U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            FI: Into<bool>,
        {
            /// Sets the field bit
            #[inline(always)]
            pub fn set_bit(self) -> &'a mut REG::Writer {
                self.bit(true)
            }
            /// Clears the field bit
            #[inline(always)]
            pub fn clear_bit(self) -> &'a mut REG::Writer {
                self.bit(false)
            }
        }
        impl<'a, REG, FI, const OF: u8> BitWriter1S<'a, $U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            FI: Into<bool>,
        {
            /// Sets the field bit
            #[inline(always)]
            pub fn set_bit(self) -> &'a mut REG::Writer {
                self.bit(true)
            }
        }
        impl<'a, REG, FI, const OF: u8> BitWriter0C<'a, $U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            FI: Into<bool>,
        {
            /// Clears the field bit
            #[inline(always)]
            pub fn clear_bit(self) -> &'a mut REG::Writer {
                self.bit(false)
            }
        }
        impl<'a, REG, FI, const OF: u8> BitWriter1C<'a, $U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            FI: Into<bool>,
        {
            ///Clears the field bit by passing one
            #[inline(always)]
            pub fn clear_bit_by_one(self) -> &'a mut REG::Writer {
                self.bit(true)
            }
        }
        impl<'a, REG, FI, const OF: u8> BitWriter0S<'a, $U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            FI: Into<bool>,
        {
            ///Sets the field bit by passing zero
            #[inline(always)]
            pub fn set_bit_by_zero(self) -> &'a mut REG::Writer {
                self.bit(false)
            }
        }
        impl<'a, REG, FI, const OF: u8> BitWriter1T<'a, $U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            FI: Into<bool>,
        {
            ///Toggle the field bit by passing one
            #[inline(always)]
            pub fn toggle_bit(self) -> &'a mut REG::Writer {
                self.bit(true)
            }
        }
        impl<'a, REG, FI, const OF: u8> BitWriter0T<'a, $U, REG, FI, OF>
        where
            REG: Writable + RegisterSpec<Ux = $U>,
            FI: Into<bool>,
        {
            ///Toggle the field bit by passing zero
            #[inline(always)]
            pub fn toggle_bit(self) -> &'a mut REG::Writer {
                self.bit(false)
            }
        }
    }
}
