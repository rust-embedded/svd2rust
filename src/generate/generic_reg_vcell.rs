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
        unsafe { REG::Reader::from_bits(self.register.get()) }
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
    pub fn write<F>(&self, f: F) -> REG::Ux
    where
        F: FnOnce(&mut REG::Writer) -> &mut REG::Writer,
    {
        let value = unsafe {
            f(&mut REG::Writer::from_bits(
                REG::RESET_VALUE & !REG::ONE_TO_MODIFY_FIELDS_BITMAP
                    | REG::ZERO_TO_MODIFY_FIELDS_BITMAP,
            ))
            .to_bits()
        };
        self.register.set(value);
        value
    }

    /// Writes bits to a `Writable` register and produce a value.
    ///
    /// You can write raw bits into a register:
    /// ```ignore
    /// periph.reg.write_and(|w| unsafe { w.bits(rawbits); });
    /// ```
    /// or write only the fields you need:
    /// ```ignore
    /// periph.reg.write_and(|w| {
    ///     w.field1().bits(newfield1bits)
    ///         .field2().set_bit()
    ///         .field3().variant(VARIANT);
    /// });
    /// ```
    /// or an alternative way of saying the same:
    /// ```ignore
    /// periph.reg.write_and(|w| {
    ///     w.field1().bits(newfield1bits);
    ///     w.field2().set_bit();
    ///     w.field3().variant(VARIANT);
    /// });
    /// ```
    /// In the latter case, other fields will be set to their reset value.
    ///
    /// Values can be returned from the closure:
    /// ```ignore
    /// let state = periph.reg.write_and(|w| State::set(w.field1()));
    /// ```
    #[inline(always)]
    pub fn from_write<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&mut REG::Writer) -> T,
    {
        let mut writer = unsafe {
            REG::Writer::from_bits(
                REG::RESET_VALUE & !REG::ONE_TO_MODIFY_FIELDS_BITMAP
                    | REG::ZERO_TO_MODIFY_FIELDS_BITMAP,
            )
        };
        let result = f(&mut writer);

        self.register.set(writer.to_bits());

        result
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
    pub unsafe fn write_with_zero<F>(&self, f: F) -> REG::Ux
    where
        F: FnOnce(&mut REG::Writer) -> &mut REG::Writer,
    {
        let value = f(&mut REG::Writer::from_bits(REG::Ux::ZERO)).to_bits();
        self.register.set(value);
        value
    }

    /// Writes 0 to a `Writable` register and produces a value.
    ///
    /// Similar to `write`, but unused bits will contain 0.
    ///
    /// # Safety
    ///
    /// Unsafe to use with registers which don't allow to write 0.
    #[inline(always)]
    pub unsafe fn from_write_with_zero<F, T>(&self, f: F) -> T
    where
        F: FnOnce(&mut REG::Writer) -> T,
    {
        let mut writer = REG::Writer::from_bits(REG::Ux::ZERO);

        let result = f(&mut writer);

        self.register.set(writer.to_bits());

        result
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
    pub fn modify<F>(&self, f: F) -> REG::Ux
    where
        for<'w> F: FnOnce(&REG::Reader, &'w mut REG::Writer) -> &'w mut REG::Writer,
    {
        let bits = self.register.get();
        let value = unsafe {
            f(
                &REG::Reader::from_bits(bits),
                &mut REG::Writer::from_bits(
                    bits & !REG::ONE_TO_MODIFY_FIELDS_BITMAP | REG::ZERO_TO_MODIFY_FIELDS_BITMAP,
                ),
            )
            .to_bits()
        };
        self.register.set(value);
        value
    }

    /// Modifies the contents of the register by reading and then writing it
    /// and produces a value.
    ///
    /// E.g. to do a read-modify-write sequence to change parts of a register:
    /// ```ignore
    /// let bits = periph.reg.modify(|r, w| {
    ///     let new_bits = r.bits() | 3;
    ///     unsafe {
    ///         w.bits(new_bits);
    ///     }
    ///
    ///     new_bits
    /// });
    /// ```
    /// or
    /// ```ignore
    /// periph.reg.modify(|_, w| {
    ///     w.field1().bits(newfield1bits)
    ///         .field2().set_bit()
    ///         .field3().variant(VARIANT);
    /// });
    /// ```
    /// or an alternative way of saying the same:
    /// ```ignore
    /// periph.reg.modify(|_, w| {
    ///     w.field1().bits(newfield1bits);
    ///     w.field2().set_bit();
    ///     w.field3().variant(VARIANT);
    /// });
    /// ```
    /// Other fields will have the value they had before the call to `modify`.
    #[inline(always)]
    pub fn from_modify<F, T>(&self, f: F) -> T
    where
        for<'w> F: FnOnce(&REG::Reader, &'w mut REG::Writer) -> T,
    {
        unsafe {
            let bits = self.register.get();

            let mut writer = REG::Writer::from_bits(
                bits & !REG::ONE_TO_MODIFY_FIELDS_BITMAP | REG::ZERO_TO_MODIFY_FIELDS_BITMAP,
            );

            let result = f(&REG::Reader::from_bits(bits), &mut writer);

            self.register.set(writer.to_bits());

            result
        }
    }
}

impl<REG: Readable> core::fmt::Debug for Reg<REG>
where
    REG::Reader: core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&self.read(), f)
    }
}
