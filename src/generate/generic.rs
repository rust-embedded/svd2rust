use quote::Tokens;

use crate::errors::*;
use crate::util::U32Ext;

/// Generates generic bit munging code
pub fn render(rsizes: &[u32]) -> Result<Vec<Tokens>> {
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

        ///Mask of field
        pub trait Mask<U> {
            ///Value of mask of field
            const MASK: U;
        }

        ///Converting enumerated values to bits
        pub trait ToBits<N> {
            ///Convertion method
            fn _bits(&self) -> N;
        }

        /// Marker trait for Enums
        pub trait Variant {}
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
            pub fn write<F>(&self, f: F)
            where
                F: FnOnce(&mut W<U, REG>) -> &mut W<U, REG>
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
            pub fn write_with_zero<F>(&self, f: F)
            where
                F: FnOnce(&mut W<U, REG>) -> &mut W<U, REG>
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
            pub fn modify<F>(&self, f: F)
            where
                for<'w> F: FnOnce(&R<U, REG>, &'w mut W<U, REG>) -> &'w mut W<U, REG>
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

        impl<U, T, FI> PartialEq<FI> for R<U, T>
        where
            T: Readable,
            U: PartialEq,
            FI: ToBits<U>
        {
            fn eq(&self, other: &FI) -> bool {
                self.bits.eq(&other._bits())
            }
        }
    });

    generic_items.push(quote! {
        impl<FI> R<bool, FI>
        where
            FI: Readable,
        {
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
        ///Register writer
        pub struct W<U, REG> where REG: Writable {
            ///Writable bits
            pub bits: U,
            _reg: marker::PhantomData<REG>,
        }

        impl<U, REG> W<U, REG> where REG: Writable {
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
        impl<U, REG> W<U, REG> where REG: Writable {
            ///Writes raw bits to the register
            #[inline(always)]
            pub unsafe fn bits(&mut self, bits: U) -> &mut Self {
                self.bits = bits;
                self
            }
        }
    });


    generic_items.push(quote! {
        ///Write Proxy
        pub struct WProxy<'a, U, REG, N, FI, S>
        where
            REG: Writable,
            FI: Writable,
        {
            w: &'a mut W<U, REG>,
            offset: u8,
            _field: marker::PhantomData<(FI, N, S)>,
        }

        impl<'a, U, REG, N, FI, S> WProxy<'a, U, REG, N, FI, S>
        where
            REG: Writable,
            FI: Writable,
        {
            pub(crate) fn new(w: &'a mut W<U, REG>, offset: u8) -> Self {
                Self {
                    w,
                    offset,
                    _field: marker::PhantomData,
                }
            }
        }
    });

    generic_items.push(quote! {
        macro_rules! impl_bit_proxy {
            ($U:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, bool, FI, Safe>
                where
                    REG: Writable,
                    FI: Writable,
                {
                    ///Sets the field bit"
                    #[inline(always)]
                    pub fn set_bit(self) -> &'a mut W<$U, REG> {
                        self.bit(true)
                    }
                    ///Clears the field bit"
                    #[inline(always)]
                    pub fn clear_bit(self) -> &'a mut W<$U, REG> {
                        self.bit(false)
                    }
                    ///Writes raw bits to the field"
                    #[inline(always)]
                    pub fn bit(self, value: bool) -> &'a mut W<$U, REG> {
                        self.w.bits &= !(0x01 << self.offset);
                        self.w.bits |= ((value as $U) & 0x01) << self.offset;
                        self.w
                    }
                }
            }
        }
    });

    generic_items.push(quote! {
        macro_rules! impl_bit_variant_proxy {
            ($U:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, bool, FI, Safe>
                where
                    REG: Writable,
                    FI: Writable + ToBits<bool> + Variant,
                {
                    ///Writes `variant` to the field
                    #[inline(always)]
                    pub fn variant(self, variant: FI) -> &'a mut W<$U, REG> {
                        self.bit(variant._bits())
                    }
                }
            }
        }
    });

    let max_rsize = rsizes.iter().max().unwrap();
    for fsize in &[8, 16, 32, 64] {
        if fsize > max_rsize {
            break;
        }
        let fty = fsize.to_ty()?;
        generic_items.push(quote! {
            impl_bit_proxy!(#fty);
            impl_bit_variant_proxy!(#fty);
        });
    }

    generic_items.push(quote! {
        macro_rules! impl_proxy_safe {
            ($U:ty, $N:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, $N, FI, Safe>
                where
                    REG: Writable,
                    FI: Writable + Mask<$U>,
                {
                    ///Writes raw bits to the field"
                    #[inline(always)]
                    pub fn bits(self, value: $N) -> &'a mut W<$U, REG> {
                        self.w.bits &= !(FI::MASK << self.offset);
                        self.w.bits |= ((value as $U) & FI::MASK) << self.offset;
                        self.w
                    }
                }
            }
        }
    });
    generic_items.push(quote! {
        macro_rules! impl_proxy_unsafe {
            ($U:ty, $N:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, $N, FI, Unsafe>
                where
                    REG: Writable,
                    FI: Writable + Mask<$U>,
                {
                    ///Writes raw bits to the field"
                    #[inline(always)]
                    pub unsafe fn bits(self, value: $N) -> &'a mut W<$U, REG> {
                        self.w.bits &= !(FI::MASK << self.offset);
                        self.w.bits |= ((value as $U) & FI::MASK) << self.offset;
                        self.w
                    }
                }
            }
        }
    });

    generic_items.push(quote! {
        macro_rules! impl_proxy_variant_safe {
            ($U:ty, $N:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, $N, FI, Safe>
                where
                    REG: Writable,
                    FI: Writable + Mask<$U> + ToBits<$N> + Variant,
                {
                    ///Writes `variant` to the field
                    #[inline(always)]
                    pub fn variant(self, variant: FI) -> &'a mut W<$U, REG> {
                        self.bits(variant._bits())
                    }
                }
            }
        }
    });

    generic_items.push(quote! {
        macro_rules! impl_proxy_variant_unsafe {
            ($U:ty, $N:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, $N, FI, Unsafe>
                where
                    REG: Writable,
                    FI: Writable + Mask<$U> + ToBits<$N> + Variant,
                {
                    ///Writes `variant` to the field
                    #[inline(always)]
                    pub fn variant(self, variant: FI) -> &'a mut W<$U, REG> {
                        unsafe { self.bits(variant._bits()) }
                    }
                }
            }
        }
    });

    for (i, rsize) in rsizes.iter().enumerate() {
        let rty = rsize.to_ty()?;
        for j in 0..=i {
            let fty = rsizes[j].to_ty()?;
            generic_items.push(quote! {
                impl_proxy_safe!(#rty, #fty);
                impl_proxy_unsafe!(#rty, #fty);
                impl_proxy_variant_safe!(#rty, #fty);
                impl_proxy_variant_unsafe!(#rty, #fty);
            });
        }
    }

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
