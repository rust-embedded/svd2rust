#![feature(plugin)]
#![feature(rustc_private)]
#![plugin(quasi_macros)]

extern crate aster;
extern crate inflections;
extern crate quasi;
extern crate svd_parser as svd;
extern crate syntax;

use syntax::ast::{Item, Ty};
use syntax::codemap::{self, ExpnInfo, MacroAttribute, NameAndSpan};
use syntax::ext::base::{DummyResolver, ExtCtxt};
use syntax::ext::expand::ExpansionConfig;
use syntax::parse::ParseSess;
use syntax::ptr::P;

use aster::AstBuilder;
use aster::name::ToName;
use inflections::Inflect;
use svd::{Access, Defaults, Peripheral, Register};

pub fn make_ext_ctxt<'a>(sess: &'a ParseSess, macro_loader: &'a mut DummyResolver) -> ExtCtxt<'a> {
    let info = ExpnInfo {
        call_site: codemap::DUMMY_SP,
        callee: NameAndSpan {
            format: MacroAttribute("_".to_name()),
            allow_internal_unstable: false,
            span: None,
        },
    };

    let cfg = Vec::new();
    let ecfg = ExpansionConfig::default(String::new());

    let mut cx = ExtCtxt::new(&sess, cfg, ecfg, macro_loader);
    cx.bt_push(info);

    cx
}

pub fn gen_peripheral(cx: &ExtCtxt, p: &Peripheral, d: &Defaults) -> Vec<P<Item>> {
    assert!(p.derived_from.is_none(), "DerivedFrom not supported");

    let mut items = vec![];
    let builder = AstBuilder::new();
    let u8 = builder.ty().u8();

    let mut fields = vec![];
    let mut offset = 0;
    let mut i = 0;
    let registers = p.registers.as_ref().expect(&format!("{:#?} has no `registers` field", p));
    for register in registers {
        let pad = register.address_offset
            .checked_sub(offset)
            .unwrap_or_else(|| panic!("{:#?} overlapped with other register!", p));

        if pad != 0 {
            fields.push(builder.struct_field(&format!("reserved{}", i))
                .ty()
                .build_array(u8.clone(), pad as usize));
            i += 1;
        }

        let comment = &format!("/// 0x{:02x} - {}",
                               register.address_offset,
                               respace(&register.description))[..];
        fields.push(builder.struct_field(register.name.to_camel_case())
            .pub_()
            .attr()
            .doc(comment)
            .ty()
            .id(register.name.to_pascal_case()));

        offset = register.address_offset +
                 register.size.or(d.size).expect(&format!("{:#?} has no `size` field", register)) /
                 8;
    }

    let struct_ = builder.item()
        .pub_()
        .attr()
        .list("repr")
        .word("C")
        .build()
        .struct_(p.name.to_pascal_case())
        .with_fields(fields)
        .build();

    if let Some(description) = p.description.as_ref() {
        let comment = &format!("/// {}", respace(description))[..];
        items.push(struct_.map(|i| {
            let mut attrs = i.attrs;
            attrs.push(builder.attr().doc(comment));
            Item { attrs: attrs, ..i }
        }))
    } else {
        items.push(struct_);
    }


    for register in registers {
        items.extend(gen_register(cx, register, d));
        items.extend(gen_register_r(cx, register, d));
        items.extend(gen_register_w(cx, register, d));
    }

    items
}

pub fn gen_register(cx: &ExtCtxt, r: &Register, d: &Defaults) -> Vec<P<Item>> {
    let builder = AstBuilder::new();
    let mut items = vec![];

    let name = builder.id(r.name.to_pascal_case());
    let bits_ty = r.size.or(d.size).expect(&format!("{:#?} has no `size` field", r)).to_ty();
    let access = r.access.unwrap_or_else(|| {
        let fields = r.fields.as_ref().expect(&format!("{:#?} has no `fields` field", r));
        if fields.iter().all(|f| f.access == Some(Access::ReadOnly)) {
            Access::ReadOnly
        } else if fields.iter().all(|f| f.access == Some(Access::WriteOnly)) {
            Access::WriteOnly
        } else if fields.iter().any(|f| f.access == Some(Access::ReadWrite)) {
            Access::ReadWrite
        } else {
            panic!("unexpected case: {:#?}",
                   fields.iter().map(|f| f.access).collect::<Vec<_>>())
        }
    });

    let name_r = builder.id(format!("{}R", r.name.to_pascal_case()));
    let name_w = builder.id(format!("{}W", r.name.to_pascal_case()));
    match access {
        Access::ReadOnly => {
            items.push(quote_item!(cx,
                                   pub struct $name {
                                       register: ::volatile_register::RO<$bits_ty>
                                   })
                .unwrap());

            items.push(quote_item!(cx,
                                   impl $name {
                                       pub fn read(&self) -> $name_r {
                                           $name_r { bits: self.register.read() }
                                       }
                                   })
                .unwrap());
        }
        Access::ReadWrite => {
            items.push(quote_item!(cx,
                                   pub struct $name {
                                       register: ::volatile_register::RW<$bits_ty>
                                   })
                .unwrap());

            items.push(quote_item!(cx,
                                   impl $name {
                                       pub fn modify<F>(&mut self, f: F)
                                           where for<'w> F: FnOnce(&$name_r, &'w mut $name_w) -> &'w mut $name_w,
                                       {
                                           let bits = self.register.read();
                                           let r = $name_r { bits: bits };
                                           let mut w = $name_w { bits: bits };
                                           f(&r, &mut w);
                                           self.register.write(w.bits);
                                       }

                                       pub fn read(&self) -> $name_r {
                                           $name_r { bits: self.register.read() }
                                       }

                                       pub fn write<F>(&mut self, f: F)
                                           where F: FnOnce(&mut $name_w) -> &mut $name_w,
                                       {
                                           let mut w = $name_w::reset_value();
                                           f(&mut w);
                                           self.register.write(w.bits);
                                       }
                                   })
                .unwrap());
        }
        Access::WriteOnly => {
            items.push(quote_item!(cx,
                                   pub struct $name {
                                       register: ::volatile_register::WO<$bits_ty>
                                   })
                .unwrap());

            items.push(quote_item!(cx,
                                   impl $name {
                                       pub fn write<F>(&self, f: F)
                                           where F: FnOnce(&mut $name_w) -> &mut $name_w,
                                       {
                                           let mut w = $name_w::reset_value();
                                           f(&mut w);
                                           self.register.write(w.bits);
                                       }
                                   })
                .unwrap());
        }
        _ => unreachable!(),
    }

    items
}

pub fn gen_register_r(cx: &ExtCtxt, r: &Register, d: &Defaults) -> Vec<P<Item>> {
    let builder = AstBuilder::new();
    let mut items = vec![];

    let name = builder.id(format!("{}R", r.name.to_pascal_case()));
    let bits_ty = r.size.or(d.size).expect(&format!("{:#?} has no `size` field", r)).to_ty();

    items.push(quote_item!(cx,
                           #[derive(Clone, Copy)]
                           pub struct $name {
                               bits: $bits_ty,
                           })
        .unwrap());

    let mut impl_items = vec![];

    for field in r.fields.as_ref().expect(&format!("{:#?} has no `fields` field", r)) {
        if let Some(Access::WriteOnly) = field.access {
            continue;
        }

        let name = builder.id(field.name.to_snake_case());
        let offset = builder.expr().lit().int(i64::from(field.bit_range.offset));

        let width = field.bit_range.width;
        let mut item = if width == 1 {
            quote_impl_item!(cx,
                             pub fn $name(&self) -> bool {
                                 const OFFSET: u8 = $offset;

                                 self.bits & (1 << OFFSET) != 0
                             })
                .unwrap()
        } else {
            let width_ty = width.to_ty();
            let mask: u32 = (1 << width) - 1;
            let mask = builder.expr().lit().int(i64::from(mask));

            quote_impl_item!(cx,
                             pub fn $name(&self) -> $width_ty {
                                 const MASK: $bits_ty = $mask;
                                 const OFFSET: u8 = $offset;

                                 ((self.bits >> OFFSET) & MASK) as $width_ty
                             })
                .unwrap()
        };

        if let Some(description) = field.description.as_ref() {
            let bits = if width == 1 {
                format!("Bit {}", field.bit_range.offset)
            } else {
                format!("Bits {}:{}", field.bit_range.offset, field.bit_range.offset + width - 1)
            };

            let comment = &format!("/// {} - {}", bits, respace(description))[..];
            item.attrs.push(builder.attr().doc(comment));
        }

        impl_items.push(item);
    }

    items.push(builder.item().impl_().with_items(impl_items).ty().id(name));

    items
}

pub fn gen_register_w(cx: &ExtCtxt, r: &Register, d: &Defaults) -> Vec<P<Item>> {
    let builder = AstBuilder::new();
    let mut items = vec![];

    let name = builder.id(format!("{}W", r.name.to_pascal_case()));
    let bits_ty = r.size.or(d.size).expect(&format!("{:#?} has no `size` field", r)).to_ty();
    items.push(quote_item!(cx,
                           #[derive(Clone, Copy)]
                           pub struct $name {
                               bits: $bits_ty,
                           })
        .unwrap());

    let mut impl_items = vec![];

    if let Some(reset_value) = r.reset_value.or(d.reset_value) {
        let reset_value = builder.expr().lit().int(i64::from(reset_value));

        impl_items.push(quote_impl_item!(cx,
                                         /// Reset value
                                         pub fn reset_value() -> Self {
                                             $name { bits: $reset_value }
                                         })
            .unwrap())
    }

    for field in r.fields.as_ref().expect(&format!("{:#?} has no `fields` field", r)) {
        if let Some(Access::ReadOnly) = field.access {
            continue;
        }

        let name = builder.id(field.name.to_snake_case());
        let offset = builder.expr().lit().int(i64::from(field.bit_range.offset));

        let width = field.bit_range.width;
        let mut item = if width == 1 {
            quote_impl_item!(cx,
                             pub fn $name(&mut self, value: bool) -> &mut Self {
                                 const OFFSET: u8 = $offset;

                                 if value {
                                     self.bits |= 1 << OFFSET;
                                 } else {
                                     self.bits &= !(1 << OFFSET);
                                 }
                                 self
                             })
                .unwrap()
        } else {
            let width_ty = width.to_ty();
            let mask: u32 = (1 << width) - 1;
            let mask = builder.expr().lit().int(i64::from(mask));

            quote_impl_item!(cx,
                             pub fn $name(&mut self, value: $width_ty) -> &mut Self {
                                 const OFFSET: u8 = $offset;
                                 const MASK: $width_ty = $mask;

                                 self.bits &= !(MASK as $bits_ty << OFFSET);
                                 self.bits |= (value & MASK) as $bits_ty << OFFSET;
                                 self
                             })
                .unwrap()
        };

        if let Some(description) = field.description.as_ref() {
            let bits = if width == 1 {
                format!("Bit {}", field.bit_range.offset)
            } else {
                format!("Bits {}:{}", field.bit_range.offset, field.bit_range.offset + width - 1)
            };

            let comment = &format!("/// {} - {}", bits, respace(description))[..];
            item.attrs.push(builder.attr().doc(comment));
        }

        impl_items.push(item);
    }

    items.push(builder.item().impl_().with_items(impl_items).ty().id(name));

    items
}

trait U32Ext {
    fn to_ty(&self) -> P<Ty>;
}

impl U32Ext for u32 {
    fn to_ty(&self) -> P<Ty> {
        let builder = AstBuilder::new();

        match *self {
            1...8 => builder.ty().u8(),
            9...16 => builder.ty().u16(),
            16...32 => builder.ty().u32(),
            _ => panic!("{}.to_ty()", *self),
        }
    }
}

fn respace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
