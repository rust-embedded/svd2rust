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
    pub mtvec_align: Option<usize>,
}

impl RiscvConfig {
    pub fn extra_build(&self) -> Option<TokenStream> {
        self.mtvec_align.map(|align| {
            quote! {
                // set environment variable RISCV_MTVEC_ALIGN enfoce correct byte alignment of interrupt vector.
                println!(
                    "cargo:rustc-env=RISCV_MTVEC_ALIGN={}",
                    #align
                );
                println!("cargo:rerun-if-env-changed=RISCV_MTVEC_ALIGN");
            }
        })
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
