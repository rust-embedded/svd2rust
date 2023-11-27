use proc_macro2::{Ident, TokenStream};
use quote::{quote, ToTokens};

#[derive(Clone, Debug)]
pub enum Accessor {
    Reg(RegAccessor),
    RawReg(RawRegAccessor),
    Array(ArrayAccessor),
    RawArray(RawArrayAccessor),
    ArrayElem(ArrayElemAccessor),
}

impl Accessor {
    pub fn raw(self) -> Self {
        match self {
            Self::RawReg(_) | Self::RawArray(_) | Self::ArrayElem(_) => self,
            Self::Reg(a) => RawRegAccessor {
                doc: a.doc,
                name: a.name,
                ty: a.ty,
                offset: a.offset,
            }
            .into(),
            Self::Array(a) => RawArrayAccessor {
                doc: a.doc,
                name: a.name,
                ty: a.ty,
                offset: a.offset,
                dim: a.dim,
                increment: a.increment,
            }
            .into(),
        }
    }
}

impl ToTokens for Accessor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Reg(a) => a.to_tokens(tokens),
            Self::RawReg(a) => a.to_tokens(tokens),
            Self::Array(a) => a.to_tokens(tokens),
            Self::RawArray(a) => a.to_tokens(tokens),
            Self::ArrayElem(a) => a.to_tokens(tokens),
        }
    }
}

impl From<RegAccessor> for Accessor {
    fn from(value: RegAccessor) -> Self {
        Self::Reg(value)
    }
}

impl From<RawRegAccessor> for Accessor {
    fn from(value: RawRegAccessor) -> Self {
        Self::RawReg(value)
    }
}

impl From<ArrayAccessor> for Accessor {
    fn from(value: ArrayAccessor) -> Self {
        Self::Array(value)
    }
}

impl From<RawArrayAccessor> for Accessor {
    fn from(value: RawArrayAccessor) -> Self {
        Self::RawArray(value)
    }
}

impl From<ArrayElemAccessor> for Accessor {
    fn from(value: ArrayElemAccessor) -> Self {
        Self::ArrayElem(value)
    }
}

#[derive(Clone, Debug)]
pub struct RegAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub offset: syn::LitInt,
}

impl ToTokens for RegAccessor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self { doc, name, ty, .. } = self;
        quote! {
            #[doc = #doc]
            #[inline(always)]
            pub const fn #name(&self) -> &#ty {
                &self.#name
            }
        }
        .to_tokens(tokens);
    }
}

#[derive(Clone, Debug)]
pub struct RawRegAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub offset: syn::LitInt,
}

impl ToTokens for RawRegAccessor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            doc,
            name,
            ty,
            offset,
        } = self;
        quote! {
            #[doc = #doc]
            #[inline(always)]
            pub const fn #name(&self) -> &#ty {
                unsafe { &*(self as *const Self).cast::<u8>().add(#offset).cast() }
            }
        }
        .to_tokens(tokens);
    }
}

#[derive(Clone, Debug)]
pub struct ArrayAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub offset: syn::LitInt,
    pub dim: syn::LitInt,
    pub increment: syn::LitInt,
}

impl ToTokens for ArrayAccessor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self { doc, name, ty, .. } = self;
        quote! {
            #[doc = #doc]
            #[inline(always)]
            pub const fn #name(&self, n: usize) -> &#ty {
                &self.#name[n]
            }
        }
        .to_tokens(tokens);
    }
}

#[derive(Clone, Debug)]
pub struct RawArrayAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub offset: syn::LitInt,
    pub dim: syn::LitInt,
    pub increment: syn::LitInt,
}

impl ToTokens for RawArrayAccessor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            doc,
            name,
            ty,
            offset,
            dim,
            increment,
        } = self;
        quote! {
            #[doc = #doc]
            #[inline(always)]
            pub const fn #name(&self, n: usize) -> &#ty {
                #[allow(clippy::no_effect)]
                [(); #dim][n];
                unsafe { &*(self as *const Self).cast::<u8>().add(#offset).add(#increment * n).cast() }
            }
        }
        .to_tokens(tokens);
    }
}

#[derive(Clone, Debug)]
pub struct ArrayElemAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub basename: Ident,
    pub i: syn::LitInt,
}

impl ToTokens for ArrayElemAccessor {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let Self {
            doc,
            name,
            ty,
            basename,
            i,
        } = &self;
        quote! {
            #[doc = #doc]
            #[inline(always)]
            pub const fn #name(&self) -> &#ty {
                self.#basename(#i)
            }
        }
        .to_tokens(tokens);
    }
}
