use quote::Tokens;
use svd::{Defaults, Peripheral};
use syn::Ident;

use errors::*;
use util::{self, ToSanitizedSnakeCase, ToSanitizedUpperCase};

pub fn render(
    p: &Peripheral,
    all_peripherals: &[Peripheral],
    defaults: &Defaults,
) -> Result<Vec<Tokens>> {
    let mut out = vec![];

    let name_pc = Ident::new(&*p.name.to_sanitized_upper_case());
    let address = util::hex(p.base_address);
    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));

    let name_sc = Ident::new(&*p.name.to_sanitized_snake_case());
    let (base, derived) = if let Some(base) = p.derived_from.as_ref() {
        // TODO Verify that base exists
        // TODO We don't handle inheritance style `derivedFrom`, we should raise
        // an error in that case
        (Ident::new(&*base.to_sanitized_snake_case()), true)
    } else {
        (name_sc.clone(), false)
    };

    out.push(quote! {
        #[doc = #description]
        pub struct #name_pc { _marker: PhantomData<*const ()> }

        unsafe impl Send for #name_pc {}

        impl #name_pc {
            /// Returns a pointer to the register block
            pub fn ptr() -> *const #base::RegisterBlock {
                #address as *const _
            }
        }

        impl Deref for #name_pc {
            type Target = #base::RegisterBlock;

            fn deref(&self) -> &#base::RegisterBlock {
                unsafe { &*#name_pc::ptr() }
            }
        }
    });

    if derived {
        return Ok(out);
    }

    let registers = p.registers.as_ref().map(|x| x.as_ref()).unwrap_or(&[][..]);

    // No `struct RegisterBlock` can be generated
    if registers.is_empty() {
        // Drop the `pub const` definition of the peripheral
        out.pop();
        return Ok(out);
    }

    let mut mod_items = vec![];
    mod_items.push(::generate::register_block(registers, defaults)?);

    for register in registers {
        ::generate::register(
            register,
            registers,
            p,
            all_peripherals,
            defaults,
            &mut mod_items,
        )?;
    }

    let description = util::respace(p.description.as_ref().unwrap_or(&p.name));
    out.push(quote! {
        #[doc = #description]
        pub mod #name_sc {
            use vcell::VolatileCell;

            #(#mod_items)*
        }
    });

    Ok(out)
}
