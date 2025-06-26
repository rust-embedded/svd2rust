use log::warn;
use proc_macro2::TokenStream;
use quote::quote;

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub struct RiscvConfig {
    pub core_interrupts: Vec<RiscvEnumItem>,
    pub exceptions: Vec<RiscvEnumItem>,
    pub priorities: Vec<RiscvEnumItem>,
    pub harts: Vec<RiscvEnumItem>,
    pub clint: Option<RiscvClintConfig>,
    pub plic: Option<RiscvPlicConfig>,
    pub base_isa: Option<String>,
    pub mtvec_align: Option<usize>,
}

impl RiscvConfig {
    pub fn extra_build(&self) -> Option<TokenStream> {
        let mut res = vec![];
        if let Some(base_isa) = self.base_isa.as_ref() {
            let base_isa = base_isa.to_lowercase();
            let rustcv_env = format!("cargo:rustc-env=RISCV_RT_BASE_ISA={base_isa}");
            res.push(quote! {
                // set environment variable RISCV_BASE_ISA to enforce correct base ISA.
                println!(#rustcv_env);
                println!("cargo:rerun-if-env-changed=RISCV_RT_BASE_ISA");
            });
        } else {
            warn!("No base RISC-V ISA specified in settings file.");
            warn!("If your target supports vectored mode, you must specify the base ISA.");
            warn!("Otherwise, `riscv-rt` macros will not provide start trap routines to core interrupt handlers");
        }
        if let Some(align) = self.mtvec_align {
            let rustcv_env = format!("cargo:rustc-env=RISCV_MTVEC_ALIGN={align}");
            res.push(quote! {
                // set environment variable RISCV_MTVEC_ALIGN enfoce correct byte alignment of interrupt vector.
                println!(#rustcv_env);
                println!("cargo:rerun-if-env-changed=RISCV_MTVEC_ALIGN");
            });
        }
        match res.is_empty() {
            true => None,
            false => Some(quote! { #(#res)* }),
        }
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub struct RiscvEnumItem {
    pub name: String,
    pub value: usize,
    pub description: Option<String>,
}

impl RiscvEnumItem {
    pub fn description(&self) -> String {
        let description = match &self.description {
            Some(d) => d,
            None => &self.name,
        };
        format!("{} - {}", self.value, description)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub struct RiscvClintConfig {
    pub pub_new: bool,
    pub name: String,
    pub mtime_freq: usize,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub struct RiscvPlicConfig {
    pub pub_new: bool,
    pub name: String,
    pub core_interrupt: Option<String>,
    pub hart_id: Option<String>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
pub enum RiscvBaseIsa {
    #[cfg_attr(feature = "serde", serde(rename = "rv32i"))]
    Rv32I,
    #[cfg_attr(feature = "serde", serde(rename = "rv32e"))]
    Rv32E,
    #[cfg_attr(feature = "serde", serde(rename = "rv64i"))]
    Rv64I,
    #[cfg_attr(feature = "serde", serde(rename = "rv64e"))]
    Rv64E,
}
