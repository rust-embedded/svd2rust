use crate::{svd::Peripheral, util, Config};
use anyhow::{anyhow, Context, Result};
use log::debug;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use svd_parser::svd::{Access, Register};

/// Whole AVR-specific generation.
///
/// SVD files for AVR devices carry no information about the configuration
/// change protection (CCP) mechanism of modern (xmega based) AVR cores, so the
/// list of protected registers and their unlock magic comes from the settings
/// file ([`crate::config::avr::AvrConfig`]). Here we translate that list into
/// `UnlockRegister` and `Protected` trait implementations for the generated
/// register types, so that protected registers can be written with
/// `write_protected`/`modify_protected` from `generic_avr_ccp.rs`.
pub fn render(peripherals: &[Peripheral], config: &Config) -> Result<TokenStream> {
    let mut mod_items = TokenStream::new();

    let Some(ccp) = config
        .settings
        .avr_config
        .as_ref()
        .and_then(|avr| avr.ccp.as_ref())
    else {
        return Ok(mod_items);
    };

    debug!("Rendering AVR configuration change protection impls");

    // The unlock register itself is not listed as protected in the SVD either;
    // its address is all the unlock sequence needs, and we can derive that
    // from the SVD instead of asking for it in the settings file.
    let (unlock_periph, unlock_reg, unlock_addr) =
        resolve_register(peripherals, &ccp.unlock_register)
            .context("can't resolve CCP unlock register")?;

    // The unlock sequence writes the magic with `out`, which can only reach
    // I/O addresses below 0x40. On every CCP-bearing core those are identical
    // to the data-space addresses the SVD describes; reject anything else so
    // a bad settings entry fails here instead of in the assembler.
    if unlock_addr >= 0x40 {
        return Err(anyhow!(
            "CCP unlock register {} is at address {:#x}, but `out` can only \
             reach I/O addresses below 0x40",
            ccp.unlock_register,
            unlock_addr
        ));
    }

    // Anchor the address on the peripheral's `PeripheralSpec::ADDRESS` const
    // instead of duplicating it as a literal; only the register's byte offset
    // is emitted directly.
    let unlock_spec = register_spec_path(unlock_periph, unlock_reg, config);
    let unlock_periph_spec = util::ident(
        &unlock_periph.name,
        config,
        "peripheral_spec",
        Span::call_site(),
    );
    let unlock_offset = util::hex(unlock_reg.address_offset as u64);

    mod_items.extend(quote! {
        impl crate::UnlockRegister for #unlock_spec {
            const ADDR: u8 =
                <#unlock_periph_spec as crate::PeripheralSpec>::ADDRESS as u8 + #unlock_offset;
        }
    });

    for protected in &ccp.protected_registers {
        let (periph, reg, _) = resolve_register(peripherals, &protected.register)
            .with_context(|| format!("can't resolve protected register {}", protected.register))?;

        // The unlock sequence in `generic_avr_ccp.rs` only supports 8-bit
        // writable registers (`RegisterSpec<Ux = u8> + Writable`); catch
        // mismatches here instead of failing later in the PAC build. Size and
        // access are usually inherited defaults on AVR SVDs, so only reject
        // explicit contradictions.
        if let Some(size) = reg.properties.size {
            if size != 8 {
                return Err(anyhow!(
                    "protected register {} is {} bits wide; only 8-bit registers are supported",
                    protected.register,
                    size
                ));
            }
        }
        if reg.properties.access == Some(Access::ReadOnly) {
            return Err(anyhow!(
                "protected register {} is read-only",
                protected.register
            ));
        }

        let reg_spec = register_spec_path(periph, reg, config);
        // Emit the magic as a hex literal so the generated code matches the
        // datasheet notation (0x9D/0xD8) instead of decimal.
        let magic = util::hex(protected.magic as u64);

        mod_items.extend(quote! {
            impl crate::Protected for #reg_spec {
                const MAGIC: u8 = #magic;
                type CcpReg = #unlock_spec;
            }
        });
    }

    Ok(mod_items)
}

/// Look up a `PERIPHERAL.REGISTER` path from the settings file in the SVD and
/// return the peripheral, the register and the register's data-space address.
///
/// Registers of derived peripherals are found on the base peripheral, but the
/// address is computed from the derived peripheral's own base address.
fn resolve_register<'a>(
    peripherals: &'a [Peripheral],
    path: &str,
) -> Result<(&'a Peripheral, &'a Register, u64)> {
    let (periph_name, reg_name) = path.split_once('.').ok_or_else(|| {
        anyhow!("register path `{path}` must have the form `PERIPHERAL.REGISTER`")
    })?;

    let periph = peripherals
        .iter()
        .find(|p| p.name == periph_name)
        .ok_or_else(|| anyhow!("no peripheral named `{periph_name}` in the SVD"))?;

    // Follow `derivedFrom` (once, as mandated by the SVD spec) in case the
    // peripheral inherits its registers from another one.
    let base = match periph.derived_from.as_deref() {
        Some(base_name) => peripherals
            .iter()
            .find(|p| p.name == base_name)
            .ok_or_else(|| {
                anyhow!("peripheral `{periph_name}` is derived from unknown `{base_name}`")
            })?,
        None => periph,
    };

    let reg = base
        .registers()
        .find(|r| r.name == reg_name)
        .ok_or_else(|| anyhow!("no register named `{reg_name}` in peripheral `{periph_name}`"))?;

    let address = periph.base_address + reg.address_offset as u64;
    Ok((periph, reg, address))
}

/// Build the module-relative path to a register's spec type, e.g.
/// `cpu::ccp::CCP_SPEC`. The impls are emitted at the top level of the device
/// module where all peripheral modules are siblings, so no crate-level prefix
/// is needed.
fn register_spec_path(periph: &Peripheral, reg: &Register, config: &Config) -> TokenStream {
    let span = Span::call_site();
    let periph_mod = util::ident(&periph.name, config, "peripheral_mod", span);
    let reg_mod = util::ident(&reg.name, config, "register_mod", span);
    let reg_spec = util::ident(&reg.name, config, "register_spec", span);
    quote! { #periph_mod::#reg_mod::#reg_spec }
}
