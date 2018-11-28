use errors::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    CortexM,
    Msp430,
    RISCV,
    None,
}

#[allow(dead_code)]
impl Target {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "cortex-m" => Target::CortexM,
            "msp430" => Target::Msp430,
            "riscv" => Target::RISCV,
            "none" => Target::None,
            _ => bail!("unknown target {}", s),
        })
    }
}
