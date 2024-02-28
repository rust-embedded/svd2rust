use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};

#[derive(Clone, Debug)]
pub enum Accessor {
    Reg(RegAccessor),
    Array(ArrayAccessor),
    ArrayElem(ArrayElemAccessor),
}

#[derive(Clone, Debug)]
pub enum AccessType {
    Ref(Accessor),
    RawRef(Accessor),
}

impl Accessor {
    pub fn raw_if(self, flag: bool) -> AccessType {
        if flag {
            AccessType::RawRef(self)
        } else {
            AccessType::Ref(self)
        }
    }
}

impl AccessType {
    pub fn raw(self) -> Self {
        match self {
            Self::RawRef(_) => self,
            Self::Ref(a) => Self::RawRef(a),
        }
    }
}

impl ToTokens for AccessType {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Ref(Accessor::Reg(RegAccessor { doc, name, ty, .. })) => {
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self) -> &#ty {
                        &self.#name
                    }
                }
            }
            Self::RawRef(Accessor::Reg(RegAccessor {
                doc,
                name,
                ty,
                offset,
            })) => {
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self) -> &#ty {
                        unsafe { &*core::ptr::from_ref(self).cast::<u8>().add(#offset).cast() }
                    }
                }
            }
            Self::Ref(Accessor::Array(ArrayAccessor { doc, name, ty, .. })) => {
                let name_iter = Ident::new(&format!("{name}_iter"), Span::call_site());
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self, n: usize) -> &#ty {
                        &self.#name[n]
                    }
                    #[doc = "Iterator for array of:"]
                    #[doc = #doc]
                    #[inline(always)]
                    pub fn #name_iter(&self) -> impl Iterator<Item=&#ty> {
                        self.#name.iter()
                    }
                }
            }
            Self::RawRef(Accessor::Array(ArrayAccessor {
                doc,
                name,
                ty,
                offset,
                dim,
                increment,
            })) => {
                let name_iter = Ident::new(&format!("{name}_iter"), Span::call_site());
                let cast = quote! { unsafe { &*core::ptr::from_ref(self).cast::<u8>().add(#offset).add(#increment * n).cast() } };
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self, n: usize) -> &#ty {
                        #[allow(clippy::no_effect)]
                        [(); #dim][n];
                        #cast
                    }
                    #[doc = "Iterator for array of:"]
                    #[doc = #doc]
                    #[inline(always)]
                    pub fn #name_iter(&self) -> impl Iterator<Item=&#ty> {
                        (0..#dim).map(move |n| #cast)
                    }
                }
            }
            Self::RawRef(Accessor::ArrayElem(elem)) | Self::Ref(Accessor::ArrayElem(elem)) => {
                let ArrayElemAccessor {
                    doc,
                    name,
                    ty,
                    basename,
                    i,
                } = elem;
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self) -> &#ty {
                        self.#basename(#i)
                    }
                }
            }
        }
        .to_tokens(tokens);
    }
}

#[derive(Clone, Debug)]
pub struct RegAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub offset: syn::LitInt,
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

#[derive(Clone, Debug)]
pub struct ArrayElemAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub basename: Ident,
    pub i: syn::LitInt,
}
