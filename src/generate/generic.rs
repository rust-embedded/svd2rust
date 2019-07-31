use quote::Tokens;

use crate::errors::*;

/// Generates generic bit munging code
pub fn render() -> Result<Vec<Tokens>> {
    let mut code = vec![];
    let mut generic_items = vec![];

    generic_items.push(quote! {
        use core::marker;

        ///Converting enumerated values to bits
        pub trait ToBits<N> {
            ///Conversion method
            fn _bits(&self) -> N;
        }
    });

    generic_items.push(quote! {
        ///Value read from the register
        pub struct FR<U, T> {
            pub(crate) bits: U,
            _reg: marker::PhantomData<T>,
        }

        impl<U, T, FI> PartialEq<FI> for FR<U, T>
        where
            U: PartialEq,
            FI: ToBits<U>
        {
            fn eq(&self, other: &FI) -> bool {
                self.bits.eq(&other._bits())
            }
        }

        impl<U, T> FR<U, T>
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
            ///Read raw bits from field
            #[inline(always)]
            pub fn bits(&self) -> U {
                self.bits
            }
        }
    });

    generic_items.push(quote! {
        impl<FI> FR<bool, FI> {
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
    });

    generic_items.push(quote! {
        ///Used if enumerated values cover not the whole range
        #[derive(Clone,Copy,PartialEq)]
        pub enum Variant<U, T> {
            ///Expected variant
            Val(T),
            ///Raw bits
            Res(U),
        }
    });

    code.push(quote! {
        #[allow(unused_imports)]
        use generic::*;
        /// Common register and bit access and modify traits
        pub mod generic {
            #(#generic_items)*
        }
    });

    Ok(code)
}
