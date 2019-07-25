use quote::Tokens;

use crate::errors::*;

/// Generates generic bit munging code
pub fn render() -> Result<Vec<Tokens>> {
    let mut code = vec![];
    let mut generic_items = vec![];

    generic_items.push(quote! {
        use core::marker;
        use core::ops::Deref;
        use vcell::VolatileCell;

        ///Marker trait for readable register/field
        pub trait Readable {}

        ///Marker trait for writable register/field
        pub trait Writable {}

        /// Marker struct for register/field with safe write
        pub struct Safe;

        /// Marker struct for register/field with unsafe write
        pub struct Unsafe;

        ///Reset value of the register
        pub trait ResetValue<U> {
            ///Reset value of the register
            fn reset_value() -> U;
        }
    });

    generic_items.push(quote! {
        ///Wrapper for registers
        pub struct Reg<REG>(pub(crate) REG);

        impl<U, REG> Reg<REG>
        where 
            REG: Readable + Deref<Target=VolatileCell<U>>,
            U: Copy
        {
            ///Reads the contents of the register
            #[inline(always)]
            pub fn read(&self) -> R<U, REG> {
                R::new((*self.0).get())
            }
        }

        impl<U, REG> Reg<REG>
        where
            Self: ResetValue<U>,
            REG: Writable + Deref<Target=VolatileCell<U>>,
            U: Copy,
        {
            ///Writes the reset value to the register
            #[inline(always)]
            pub fn reset(&self) {
                (*self.0).set(Self::reset_value())
            }
        }
    });

    generic_items.push(quote! {
        impl<U, REG> Reg<REG>
        where
            Self: ResetValue<U>,
            REG: Writable + Deref<Target=VolatileCell<U>>,
            U: Copy
        {
            ///Writes to the register
            #[inline(always)]
            pub fn write<F, S>(&self, f: F)
            where
                F: FnOnce(&mut W<U, REG, S>) -> &mut W<U, REG, S>
            {
                
                (*self.0).set(f(&mut W::new(Self::reset_value())).bits);
            }
        }
    });

    generic_items.push(quote! {
        impl<U, REG> Reg<REG>
        where
            REG: Writable + Deref<Target=VolatileCell<U>>,
            U: Copy + Default
        {
            ///Writes Zero to the register
            #[inline(always)]
            pub fn write_with_zero<F, S>(&self, f: F)
            where
                F: FnOnce(&mut W<U, REG, S>) -> &mut W<U, REG, S>
            {
                
                (*self.0).set(f(&mut W::new(U::default())).bits);
            }
        }
    });

    generic_items.push(quote! {
        impl<U, REG> Reg<REG>
        where
            REG: Readable + Writable + Deref<Target = VolatileCell<U>>,
            U: Copy,
        {
            ///Modifies the contents of the register
            #[inline(always)]
            pub fn modify<F, S>(&self, f: F)
            where
                for<'w> F: FnOnce(&R<U, REG>, &'w mut W<U, REG, S>) -> &'w mut W<U, REG, S>
            {
                let bits = (*self.0).get();
                (*self.0).set(f(&R::new(bits), &mut W::new(bits)).bits);
            }
        }
    });

    generic_items.push(quote! {
        ///Register reader
        pub struct R<U, T> where T: Readable {
            bits: U,
            _reg: marker::PhantomData<T>,
        }

        impl<U, T> R<U, T>
        where
            T: Readable,
            U: Copy
        {
            ///Create new instance of reader
            #[inline(always)]
            pub fn new(bits: U) -> Self {
                Self {
                    bits,
                    _reg: marker::PhantomData,
                }
            }
            ///Read raw bits from register
            #[inline(always)]
            pub fn bits(&self) -> U {
                self.bits
            }
        }
    });

    generic_items.push(quote! {
        ///Register writer
        pub struct W<U, REG, S> where REG: Writable {
            ///Writable bits
            pub bits: U,
            _reg: marker::PhantomData<(REG, S)>,
        }

        impl<U, REG, S> W<U, REG, S> where REG: Writable {
            ///Create new instance of reader
            #[inline(always)]
            pub(crate) fn new(bits: U) -> Self {
                Self {
                    bits,
                    _reg: marker::PhantomData,
                }
            }
        }
    });

    generic_items.push(quote! {
        impl<U, REG> W<U, REG, Safe> where REG: Writable {
            ///Writes raw bits to the register
            #[inline(always)]
            pub fn bits(&mut self, bits: U) -> &mut Self {
                self.bits = bits;
                self
            }
        }

        impl<U, REG> W<U, REG, Unsafe> where REG: Writable {
            ///Writes raw bits to the register
            #[inline(always)]
            pub unsafe fn bits(&mut self, bits: U) -> &mut Self {
                self.bits = bits;
                self
            }
        }
    });

    code.push(quote! {
        #[allow(unused_imports)]
        use generic::*;
        ///Common register and bit access and modify traits
        pub mod generic {
            #(#generic_items)*
        }
    });

    Ok(code)
}
