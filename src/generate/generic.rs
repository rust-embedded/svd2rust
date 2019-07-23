use quote::Tokens;

use crate::errors::*;
use crate::util::U32Ext;

/// Generates generic bit munging code
pub fn render(rsizes: &[u32]) -> Result<Vec<Tokens>> {
    let mut code = vec![];
    let mut generic_items = vec![];

    generic_items.push(quote! {
        use core::marker::PhantomData;
        use core::ops::Deref;
        use vcell::VolatileCell;

        ///Marker trait for writable register 
        pub trait Writable {}

        ///Marker trait for readable register 
        pub trait Readable {}

        ///Reset value of the register
        pub trait ResetValue<U> {
            /// Reset value of the register
            fn reset_value() -> U;
        }

        ///Field offset
        pub trait Offset {
            const OFFSET: u8;
        }

        ///Mask of field
        pub trait Mask<U> {
            const MASK: U;
        }

        ///Marker struct for field with safe write
        pub struct Safe;
        ///Marker struct for field with unsafe write
        pub struct Unsafe;

        pub trait ToBits<N> {
            fn _bits(&self) -> N;
        }

        /// Marker trait for Enums
        pub trait Variant {}
    });

    generic_items.push(quote! {
        ///Value read from the register
        pub struct R<U, T> where T: Readable {
            bits: U,
            _reg: PhantomData<T>,
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

        impl<U, T> R<U, T>
        where
            T: Readable,
            U: Copy
        {
            #[inline(always)]
            pub(crate) fn new(bits: U) -> Self {
                Self {
                    bits,
                    _reg: PhantomData,
                }
            }
            #[inline(always)]
            pub fn bits(&self) -> U {
                self.bits
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
        ///Value to write to the register
        pub struct W<U, REG> where REG: Writable {
            pub(crate) bits: U,
            _reg: PhantomData<REG>,
        }

        impl<U, REG> W<U, REG> where REG: Writable {
            #[inline(always)]
            pub(crate) fn new(bits: U) -> Self {
                Self {
                    bits,
                    _reg: PhantomData,
                }
            }
        }

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
        ///Writer Proxy
        pub struct WProxy<'a, U, REG, N, FI, S>
        where
            REG: Writable,
            FI: Writable,
        {
            w: &'a mut W<U, REG>,
            _field: PhantomData<(FI, N, S)>,
        }

        impl<'a, U, REG, N, FI, S> WProxy<'a, U, REG, N, FI, S>
        where
            REG: Writable,
            FI: Writable,
        {
            pub(crate) fn new(w: &'a mut W<U, REG>) -> Self {
                Self {
                    w,
                    _field: PhantomData,
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
                    FI: Writable + Offset,
                {
                    ///Sets the field bit"]
                    pub fn set_bit(self) -> &'a mut W<$U, REG> {
                        self.bit(true)
                    }
                    ///Clears the field bit"]
                    pub fn clear_bit(self) -> &'a mut W<$U, REG> {
                        self.bit(false)
                    }
                    ///Writes raw bits to the field"]
                    #[inline(always)]
                    pub fn bit(self, value: bool) -> &'a mut W<$U, REG> {
                        self.w.bits &= !(0x01 << FI::OFFSET);
                        self.w.bits |= ((value as $U) & 0x01) << FI::OFFSET;
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
                    FI: Writable + Offset + ToBits<bool> + Variant,
                {
                    ///Writes `variant` to the field
                    #[inline]
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
            ($U:ty | $N:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, $N, FI, Safe>
                where
                    REG: Writable,
                    FI: Writable + Mask<$U> + Offset,
                {
                    ///Writes raw bits to the field"]
                    #[inline]
                    pub fn bits(self, value: $N) -> &'a mut W<$U, REG> {
                        self.w.bits &= !(FI::MASK << FI::OFFSET);
                        self.w.bits |= ((value as $U) & FI::MASK) << FI::OFFSET;
                        self.w
                    }
                }
            }
        }
    });
    generic_items.push(quote! {
        macro_rules! impl_proxy_unsafe {
            ($U:ty | $N:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, $N, FI, Unsafe>
                where
                    REG: Writable,
                    FI: Writable + Mask<$U> + Offset,
                {
                    ///Writes raw bits to the field"]
                    #[inline]
                    pub unsafe fn bits(self, value: $N) -> &'a mut W<$U, REG> {
                        self.w.bits &= !(FI::MASK << FI::OFFSET);
                        self.w.bits |= ((value as $U) & FI::MASK) << FI::OFFSET;
                        self.w
                    }
                }
            }
        }
    });

    generic_items.push(quote! {
        macro_rules! impl_proxy_variant_safe {
            ($U:ty | $N:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, $N, FI, Safe>
                where
                    REG: Writable,
                    FI: Writable + Mask<$U> + Offset + ToBits<$N> + Variant,
                {
                    ///Writes `variant` to the field
                    #[inline]
                    pub fn variant(self, variant: FI) -> &'a mut W<$U, REG> {
                        self.bits(variant._bits())
                    }
                }
            }
        }
    });

    generic_items.push(quote! {
        macro_rules! impl_proxy_variant_unsafe {
            ($U:ty | $N:ty) => {
                impl<'a, REG, FI> WProxy<'a, $U, REG, $N, FI, Unsafe>
                where
                    REG: Writable,
                    FI: Writable + Mask<$U> + Offset + ToBits<$N> + Variant,
                {
                    ///Writes `variant` to the field
                    #[inline]
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
                impl_proxy_safe!(#rty | #fty);
                impl_proxy_unsafe!(#rty | #fty);
                impl_proxy_variant_safe!(#rty | #fty);
                impl_proxy_variant_unsafe!(#rty | #fty);
            });
        }
    }

    generic_items.push(quote! {
        ///Writes the reset value to the register
        pub trait ResetRegister<U>: Writable + ResetValue<U>
        where
            U: Copy,
        {
            ///Writes the reset value to the register
            fn reset(&self);
        }

        ///Writes the reset value to the register
        impl<U, REG> ResetRegister<U> for REG
        where
            REG: Writable + ResetValue<U> + Deref<Target=vcell::VolatileCell<U>>,
            U: Copy,
        {
            ///Writes the reset value to the register
            #[inline(always)]
            fn reset(&self) {
                (*self).set(Self::reset_value())
            }
        }
    });

    generic_items.push(quote! {
        ///Reads the contents of the register
        pub trait ReadRegister<U, REG>
        where
            REG: Readable + Deref<Target=VolatileCell<U>>
        {
            ///Reads the contents of the register
            fn read(&self) -> R<U, REG>;
        }

        impl<U, REG> ReadRegister<U, REG> for REG
        where 
            REG: Readable + Deref<Target=vcell::VolatileCell<U>>,
            U: Copy
        {
            #[inline(always)]
            fn read(&self) -> R<U, REG> {
                R::new((*self).get())
            }
        }
    });

    generic_items.push(quote! {
        /// Writes to the register using `reset_value` as basis
        pub trait WriteRegister<U, REG>
        where
            REG: Writable + ResetValue<U> + Deref<Target=VolatileCell<U>>
        {
            ///Writes to the register
            fn write<F>(&self, f: F)
            where
                F: FnOnce(&mut W<U, REG>) -> &mut W<U, REG>;
        }

        impl<U, REG> WriteRegister<U, REG> for REG
        where
            REG: Writable + ResetValue<U> + Deref<Target=VolatileCell<U>>,
            U: Copy
        {
            ///Writes to the register
            #[inline(always)]
            fn write<F>(&self, f: F)
            where
                F: FnOnce(&mut W<U, REG>) -> &mut W<U, REG>
            {
                
                (*self).set(f(&mut W::new(Self::reset_value())).bits);
            }
        }
    });

    generic_items.push(quote! {
        ///Writes to the register using Zero as basis
        pub trait WriteRegisterWithZero<U, REG>
        where
            REG: Writable + Deref<Target=VolatileCell<U>>,
            U: Default
        {
            ///Writes to the register
            fn write_with_zero<F>(&self, f: F)
            where
                F: FnOnce(&mut W<U, REG>) -> &mut W<U, REG>;
        }

        impl<U, REG> WriteRegisterWithZero<U, REG> for REG
        where
            REG: Writable + Deref<Target=VolatileCell<U>>,
            U: Copy + Default
        {
            ///Writes to the register
            #[inline(always)]
            fn write_with_zero<F>(&self, f: F)
            where
                F: FnOnce(&mut W<U, REG>) -> &mut W<U, REG>
            {
                
                (*self).set(f(&mut W::new(U::default())).bits);
            }
        }
    });

    generic_items.push(quote! {
        ///Modifies the contents of the register
        pub trait ModifyRegister<U, REG>
        where
            REG: Readable + Writable + Deref<Target = VolatileCell<U>>
        {
            ///Modifies the contents of the register
            fn modify<F>(&self, f: F)
            where
                for<'w> F: FnOnce(&R<U, REG>, &'w mut W<U, REG>) -> &'w mut W<U, REG>;
        }

        impl<U, REG> ModifyRegister<U, REG> for REG
        where
            REG: Readable + Writable + Deref<Target = VolatileCell<U>>,
            U: Copy,
        {
            ///Modifies the contents of the register
            #[inline(always)]
            fn modify<F>(&self, f: F)
            where
                for<'w> F: FnOnce(&R<U, REG>, &'w mut W<U, REG>) -> &'w mut W<U, REG>
            {
                let bits = (*self).get();
                (*self).set(f(&R::new(bits), &mut W::new(bits)).bits);
            }
        }
    });

    generic_items.push(quote! {
        #[macro_export]
        macro_rules! impl_deref {
            ($U:ty, $REG:ty) => {
                impl core::ops::Deref for $REG {
                    type Target = vcell::VolatileCell<$U>;
                    fn deref(&self) -> &Self::Target {
                        &self.register
                    }
                }
            }
        }
    });

    generic_items.push(quote! {
        #[macro_export]
        macro_rules! impl_read {
            ($U:ty, $REG:ty) => {
                impl crate::Readable for $REG {}
                impl $REG {
                    #[inline(always)]
                    pub fn read(&self) -> _R {
                        <Self as crate::ReadRegister<$U, Self>>::read(self)
                    }
                }
            }
        }
    });

    generic_items.push(quote! {
        #[macro_export]
        macro_rules! impl_write {
            ($U:ty, $REG:ty) => {
                impl crate::Writable for $REG {}
                impl $REG {
                    #[inline(always)]
                    pub fn write<F>(&self, f: F)
                    where
                        F: FnOnce(&mut _W) -> &mut _W
                    {
                        <Self as crate::WriteRegister<$U, Self>>::write(self, f)
                    }
                    #[inline(always)]
                    pub fn write_with_zero<F>(&self, f: F)
                    where
                        F: FnOnce(&mut _W) -> &mut _W
                    {
                        <Self as crate::WriteRegisterWithZero<$U, Self>>::write_with_zero(self, f)
                    }
                    #[inline(always)]
                    pub fn reset(&self) {
                        <Self as crate::ResetRegister<$U>>::reset(self)
                    }
                }
            }
        }
    });

    generic_items.push(quote! {
        #[macro_export]
        macro_rules! impl_modify {
            ($U:ty, $REG:ty) => {
                impl $REG {
                    /// Modifies the contents of the register
                    #[inline(always)]
                    pub fn modify<F>(&self, f: F)
                    where
                        for<'w> F: FnOnce(&_R, &'w mut _W) -> &'w mut _W
                    {
                            <Self as crate::ModifyRegister<$U, Self>>::modify(self, f)
                    }
                }
            }
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
