use core::marker;

///This trait shows that register has `read` method
///
///Registers marked with `Writable` can be also `modify`'ed
pub trait Readable {}

///This trait shows that register has `write`, `write_with_zero` and `reset` method
///
///Registers marked with `Readable` can be also `modify`'ed
pub trait Writable {}

///Reset value of the register
///
///This value is initial value for `write` method.
///It can be also directly writed to register by `reset` method.
pub trait ResetValue {
    ///Register size
    type Type;
    ///Reset value of the register
    fn reset_value() -> Self::Type;
}

///Converting enumerated values to bits
pub trait ToBits<N> {
    ///Conversion method
    fn _bits(&self) -> N;
}

///This structure provides volatile access to register
pub struct Reg<U, REG> {
    register: vcell::VolatileCell<U>,
    _marker: marker::PhantomData<REG>,
}

unsafe impl<U: Send, REG> Send for Reg<U, REG> { }

impl<U, REG> Reg<U, REG>
where
    Self: Readable,
    U: Copy
{
    ///Reads the contents of `Readable` register
    ///
    ///See [reading](https://rust-embedded.github.io/book/start/registers.html#reading) in book.
    #[inline(always)]
    pub fn read(&self) -> R<U, Self> {
        R {bits: self.register.get(), _reg: marker::PhantomData}
    }
}

impl<U, REG> Reg<U, REG>
where
    Self: ResetValue<Type=U> + Writable,
    U: Copy,
{
    ///Writes the reset value to `Writable` register
    #[inline(always)]
    pub fn reset(&self) {
        self.register.set(Self::reset_value())
    }
}

impl<U, REG> Reg<U, REG>
where
    Self: ResetValue<Type=U> + Writable,
    U: Copy
{
    ///Writes bits to `Writable` register
    ///
    ///See [writing](https://rust-embedded.github.io/book/start/registers.html#writing) in book.
    #[inline(always)]
    pub fn write<F>(&self, f: F)
    where
        F: FnOnce(&mut W<U, Self>) -> &mut W<U, Self>
    {
        self.register.set(f(&mut W {bits: Self::reset_value(), _reg: marker::PhantomData}).bits);
    }
}

impl<U, REG> Reg<U, REG>
where
    Self: Writable,
    U: Copy + Default
{
    ///Writes Zero to `Writable` register
    #[inline(always)]
    pub fn write_with_zero<F>(&self, f: F)
    where
        F: FnOnce(&mut W<U, Self>) -> &mut W<U, Self>
    {
        self.register.set(f(&mut W {bits: U::default(), _reg: marker::PhantomData }).bits);
    }
}

impl<U, REG> Reg<U, REG>
where
    Self: Readable + Writable,
    U: Copy,
{
    ///Modifies the contents of the register
    ///
    ///See [modifying](https://rust-embedded.github.io/book/start/registers.html#modifying) in book.
    #[inline(always)]
    pub fn modify<F>(&self, f: F)
    where
        for<'w> F: FnOnce(&R<U, Self>, &'w mut W<U, Self>) -> &'w mut W<U, Self>
    {
        let bits = self.register.get();
        self.register.set(f(&R {bits, _reg: marker::PhantomData}, &mut W {bits, _reg: marker::PhantomData}).bits);
    }
}

///Register/field reader
pub struct R<U, T> {
    pub(crate) bits: U,
    _reg: marker::PhantomData<T>,
}

impl<U, T> R<U, T>
where
    U: Copy
{
    ///Create new instance of reader
    #[inline(always)]
    pub(crate) fn new(bits: U) -> Self {
        Self {
            bits,
            _reg: marker::PhantomData,
        }
    }
    ///Read raw bits from register/field
    #[inline(always)]
    pub fn bits(&self) -> U {
        self.bits
    }
}

impl<U, T, FI> PartialEq<FI> for R<U, T>
where
    U: PartialEq,
    FI: ToBits<U>
{
    fn eq(&self, other: &FI) -> bool {
        self.bits.eq(&other._bits())
    }
}

impl<FI> R<bool, FI> {
    ///Value of the field as raw bits
    #[inline(always)]
    pub fn bit(&self) -> bool {
        self.bits
    }
    ///Returns `true` if the bit is clear (0)
    #[inline(always)]
    pub fn bit_is_clear(&self) -> bool {
        !self.bit()
    }
    ///Returns `true` if the bit is set (1)
    #[inline(always)]
    pub fn bit_is_set(&self) -> bool {
        self.bit()
    }
}

///Register writer
pub struct W<U, REG> {
    ///Writable bits
    pub bits: U,
    _reg: marker::PhantomData<REG>,
}

impl<U, REG> W<U, REG> {
    ///Writes raw bits to the register
    #[inline(always)]
    pub fn bits(&mut self, bits: U) -> &mut Self {
        self.bits = bits;
        self
    }
}

///Used if enumerated values cover not the whole range
#[derive(Clone,Copy,PartialEq)]
pub enum Variant<U, T> {
    ///Expected variant
    Val(T),
    ///Raw bits
    Res(U),
}
