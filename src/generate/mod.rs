use svd::WriteConstraint;
use syn::Ident;

// use errors::*;

pub mod device;
pub mod interrupt;
pub mod peripheral;
pub mod register;

fn unsafety(write_constraint: Option<&WriteConstraint>, width: u32) -> Option<Ident> {
    match write_constraint {
        Some(&WriteConstraint::Range(ref range))
            if range.min as u64 == 0 && range.max as u64 == (1u64 << width) - 1 =>
        {
            // the SVD has acknowledged that it's safe to write
            // any value that can fit in the field
            None
        }
        None if width == 1 => {
            // the field is one bit wide, so we assume it's legal to write
            // either value into it or it wouldn't exist; despite that
            // if a writeConstraint exists then respect it
            None
        }
        _ => Some(Ident::new("unsafe")),
    }
}
