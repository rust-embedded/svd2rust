#[derive(Clone, Copy, PartialEq)]
pub enum Target {
    CortexM,
    Msp430,
    RISCV,
    None,
}
