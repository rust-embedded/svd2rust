use core::marker;

/// Raw register type
pub trait RegisterSpec {
    /// Raw register type (`u8`, `u16`, `u32`, ...).
    type Ux: Copy;
}

/// Trait implemented by readable registers to enable the `read` method.
///
/// Registers marked with `Writable` can be also `modify`'ed.
pub trait Readable: RegisterSpec {
    /// Result from a call to `read` and argument to `modify`.
    type Reader: core::convert::From<R<Self>> + core::ops::Deref<Target = R<Self>>;
}

/// Trait implemented by writeable registers.
///
/// This enables the  `write`, `write_with_zero` and `reset` methods.
///
/// Registers marked with `Readable` can be also `modify`'ed.
pub trait Writable: RegisterSpec {
    /// Writer type argument to `write`, et al.
    type Writer: core::convert::From<W<Self>> + core::ops::DerefMut<Target = W<Self>>;
}

/// Reset value of the register.
///
/// This value is the initial value for the `write` method. It can also be directly written to the
/// register by using the `reset` method.
pub trait Resettable: RegisterSpec {
    /// Reset value of the register.
    fn reset_value() -> Self::Ux;
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
        self.register.set(REG::reset_value())
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
    /// In the latter case, other fields will be set to their reset value.
    #[inline(always)]
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut REG::Writer) -> &mut W<REG>
    {
        self.register.set(
            f(&mut REG::Writer::from(W {
                bits: REG::reset_value(),
                _reg: marker::PhantomData,
            }))
            .bits,
        );
    }
}

impl<REG: Writable> Reg<REG>
where
    REG::Ux: Default,
{
    /// Writes 0 to a `Writable` register.
    ///
    /// Similar to `write`, but unused bits will contain 0.
    #[inline(always)]
    pub fn write_with_zero<F>(&self, f: F)
    where
        F: FnOnce(&mut REG::Writer) -> &mut W<REG>
    {
        self.register.set(
            (*f(&mut REG::Writer::from(W {
                bits: REG::Ux::default(),
                _reg: marker::PhantomData,
            })))
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
                    bits,
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
    #[inline(always)]
    pub unsafe fn bits(&mut self, bits: REG::Ux) -> &mut Self {
        self.bits = bits;
        self
    }
}

/// Used if enumerated values cover not the whole range.
#[derive(Clone, Copy, PartialEq)]
pub enum Variant<U, T> {
    /// Expected variant.
    Val(T),
    /// Raw bits.
    Res(U),
}

/// Field reader.
///
/// Result of the `read` methods of fields.
pub struct FieldReader<U, T> {
    pub(crate) bits: U,
    _reg: marker::PhantomData<T>,
}

impl<U, T> FieldReader<U, T>
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

    /// Reads raw bits from field.
    #[inline(always)]
    pub fn bits(&self) -> U {
        self.bits
    }
}

impl<U, T, FI> PartialEq<FI> for FieldReader<U, T>
where
    U: PartialEq,
    FI: Copy + Into<U>,
{
    #[inline(always)]
    fn eq(&self, other: &FI) -> bool {
        self.bits.eq(&(*other).into())
    }
}

impl<FI> FieldReader<bool, FI> {
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
