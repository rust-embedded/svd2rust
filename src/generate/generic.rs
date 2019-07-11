use quote::Tokens;

use crate::errors::*;

/// Generates generic bit munging code
pub fn render() -> Result<Vec<Tokens>> {
    let mut code = vec![];

    code.push(quote! {
        #[macro_export]
        macro_rules! deref_cell {
            ($REG:ty, $T:ty) => {
                impl core::ops::Deref for $REG {
                    type Target = vcell::VolatileCell<$T>;
                    fn deref(&self) -> &Self::Target {
                        &self.register
                    }
                }
            }
        }

        #[macro_export]
        macro_rules! w {
            ($T:ty) => {
                pub struct W {
                    bits: $T,
                }

                impl core::ops::Deref for W {
                    type Target = $T;
                    fn deref(&self) -> &Self::Target {
                        &self.bits
                    }
                }

                impl ra::FromBits<$T> for W {
                    fn from_bits(bits: $T) -> Self {
                        Self { bits }
                    }
                }
            }
        }

        #[macro_export]
        macro_rules! r {
            ($T:ty) => {
                pub struct R {
                    bits: $T,
                }

                impl ra::FromBits<$T> for R {
                    fn from_bits(bits: $T) -> Self {
                        Self { bits }
                    }
                }
            }
        }
    });

    Ok(code)
}
