/// AVR specific configuration
///
/// The SVD files for AVR devices do not contain any information about the
/// configuration change protection (CCP) mechanism found on modern (xmega
/// based) AVR cores: neither which register unlocks protected writes, nor
/// which registers are protected by it. This configuration fills that gap.
#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub struct AvrConfig {
    /// Configuration change protection (CCP) description for this device
    pub ccp: Option<CcpConfig>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub struct CcpConfig {
    /// `PERIPHERAL.REGISTER` path of the register that unlocks protected
    /// writes when the magic value is written to it, e.g. `CPU.CCP`
    pub unlock_register: String,
    /// All registers of the device that are configuration change protected
    pub protected_registers: Vec<CcpProtectedRegister>,
}

#[cfg_attr(feature = "serde", derive(serde::Deserialize), serde(default))]
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub struct CcpProtectedRegister {
    /// `PERIPHERAL.REGISTER` path of the protected register, e.g.
    /// `NVMCTRL.CTRLA`
    pub register: String,
    /// Magic value that must be written to the unlock register to allow
    /// writing this register, e.g. `0x9D` (SPM) or `0xD8` (IOREG)
    pub magic: u8,
}
