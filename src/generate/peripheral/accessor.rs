use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};

use crate::util::unsuffixed;

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
    Ptr(Accessor),
}

impl Accessor {
    pub fn ptr_or_rawref_if(self, ptr_flag: bool, raw_flag: bool) -> AccessType {
        if ptr_flag {
            AccessType::Ptr(self)
        } else if raw_flag {
            AccessType::RawRef(self)
        } else {
            AccessType::Ref(self)
        }
    }
}

impl AccessType {
    pub fn raw(self) -> Self {
        match self {
            Self::RawRef(_) | Self::Ptr(_) => self,
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
                let offset = (*offset != 0).then(|| unsuffixed(*offset)).map(|o| quote!(.add(#o)));
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self) -> &#ty {
                        unsafe { &*core::ptr::from_ref(self).cast::<u8>() #offset .cast() }
                    }
                }
            }
            Self::Ptr(Accessor::Reg(RegAccessor {
                doc,
                name,
                ty,
                offset,
            })) => {
                let offset = (*offset != 0).then(|| unsuffixed(*offset)).map(|o| quote!(.add(#o)));
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self) -> #ty {
                        #ty::new(unsafe { self.ptr() #offset })
                    }
                }
            }
            Self::Ref(Accessor::Array(ArrayAccessor { doc, name, ty, note, .. })) => {
                let name_iter = Ident::new(&format!("{name}_iter"), Span::call_site());
                let note = note.as_ref().map(|note| quote! {
                    #[doc = ""]
                    #[doc = #note]
                });
                quote! {
                    #[doc = #doc]
                    #note
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
                note,
            })) => {
                let name_iter = Ident::new(&format!("{name}_iter"), Span::call_site());
                let offset = (*offset != 0).then(|| unsuffixed(*offset)).map(|o| quote!(.add(#o)));
                let dim = unsuffixed(*dim);
                let increment = (*increment != 1).then(|| unsuffixed(*increment)).map(|i| quote!(#i *));
                let note = note.as_ref().map(|note| quote! {
                    #[doc = ""]
                    #[doc = #note]
                });
                let cast = quote! { unsafe { &*core::ptr::from_ref(self).cast::<u8>() #offset .add(#increment n).cast() } };
                quote! {
                    #[doc = #doc]
                    #note
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
            Self::Ptr(Accessor::Array(ArrayAccessor {
                doc,
                name,
                ty,
                offset,
                dim,
                increment,
                note,
            })) => {
                let name_iter = Ident::new(&format!("{name}_iter"), Span::call_site());
                let offset = (*offset != 0).then(|| unsuffixed(*offset)).map(|o| quote!(.add(#o)));
                let dim = unsuffixed(*dim);
                let increment = (*increment != 1).then(|| unsuffixed(*increment)).map(|i| quote!(#i *));
                let note = note.as_ref().map(|note| quote! {
                    #[doc = ""]
                    #[doc = #note]
                });
                let cast = quote! { #ty::new(unsafe { self.ptr() #offset .add(#increment n) }) };
                quote! {
                    #[doc = #doc]
                    #note
                    #[inline(always)]
                    pub const fn #name(&self, n: usize) -> #ty {
                        #[allow(clippy::no_effect)]
                        [(); #dim][n];
                        #cast
                    }
                    #[doc = "Iterator for array of:"]
                    #[doc = #doc]
                    #[inline(always)]
                    pub fn #name_iter(&self) -> impl Iterator<Item=#ty> + '_ {
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
                let i = unsuffixed(*i as u64);
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self) -> &#ty {
                        self.#basename(#i)
                    }
                }
            }
            Self::Ptr(Accessor::ArrayElem(ArrayElemAccessor {
                    doc,
                    name,
                    ty,
                    basename,
                    i,
                })) => {
                quote! {
                    #[doc = #doc]
                    #[inline(always)]
                    pub const fn #name(&self) -> #ty {
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
    pub offset: u32,
}

#[derive(Clone, Debug)]
pub struct ArrayAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub offset: u32,
    pub dim: u32,
    pub increment: u32,
    pub note: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ArrayElemAccessor {
    pub doc: String,
    pub name: Ident,
    pub ty: syn::Type,
    pub basename: Ident,
    pub i: usize,
}
