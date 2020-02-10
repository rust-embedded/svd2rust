use crate::quote::ToTokens;
use proc_macro2::{Ident, Punct, Spacing, Span, TokenStream};
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub enum Item {
    Tokens(TokenStream),
    Module(Module),
}

/// Structure of module:
///
/// | #out
/// |
/// | /// #description
/// | mod #name {
/// |    #items
/// | }
pub struct Module {
    name: String,
    description: String,
    pub out: TokenStream,
    items: Vec<Item>,
}

impl Module {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            out: TokenStream::new(),
            items: Vec::new(),
        }
    }
    pub fn push_module(&mut self, module: Module) {
        self.items.push(Item::Module(module))
    }
    pub fn extend(&mut self, tokens: TokenStream) {
        self.items.push(Item::Tokens(tokens))
    }
    pub fn into_token_stream(self) -> TokenStream {
        let mut tokens = self.out;
        let open = Punct::new('{', Spacing::Alone);
        let close = Punct::new('}', Spacing::Alone);
        let name = Ident::new(&self.name, Span::call_site());
        let description = self.description;
        if !self.items.is_empty() {
            tokens.extend(quote! {
                #[doc = #description]
                pub mod #name #open
            });
            for item in self.items.into_iter() {
                tokens.extend(match item {
                    Item::Tokens(t) => t,
                    Item::Module(m) => m.into_token_stream(),
                });
            }
            close.to_tokens(&mut tokens);
        }
        tokens
    }
    pub fn items_into_token_stream(self) -> TokenStream {
        let mut tokens = TokenStream::new();
        for item in self.items.into_iter() {
            tokens.extend(match item {
                Item::Tokens(t) => t,
                Item::Module(m) => m.into_token_stream(),
            });
        }
        tokens
    }
    #[allow(dead_code)]
    fn items_to_files(items: Vec<Item>, path: &Path, dir_path: &Path) {
        let mut tokens = TokenStream::new();
        std::fs::create_dir_all(&dir_path)
            .expect(&format!("Could not create directory {:?}", dir_path));
        for item in items.into_iter() {
            tokens.extend(match item {
                Item::Tokens(t) => t,
                Item::Module(m) => m.to_files(&dir_path),
            });
            let mut file = File::create(path).unwrap();
            file.write_all(tokens.to_string().as_ref())
                .expect(&format!("Could not write code to {:?}", path));
        }
    }
    #[allow(dead_code)]
    pub fn to_files(self, path: &Path) -> TokenStream {
        let Module {
            name,
            description,
            mut out,
            items,
        } = self;
        if !items.is_empty() {
            Self::items_to_files(
                items,
                &path.join(&format!("{}.rs", name)),
                &path.join(&name),
            );
            let name = Ident::new(&name, Span::call_site());
            out.extend(quote! {
                #[doc = #description]
                pub mod #name;
            });
        }
        out
    }
    #[allow(dead_code)]
    pub fn lib_to_files(self, path: &Path) {
        if path.exists() {
            std::fs::remove_dir_all(path).unwrap();
        }
        if !self.items.is_empty() {
            Self::items_to_files(self.items, &path.join("lib.rs"), path);
        }
    }
}
