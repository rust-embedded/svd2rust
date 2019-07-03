use quote::Tokens;

use crate::errors::*;

/// Generates generic bit munging code
pub fn render() -> Result<Vec<Tokens>> {
    let mut code = vec![];

    code.push(quote! {
        /// Single bit read access proxy
        trait BitR {
            #[doc = r" Returns `true` if the bit is clear (0)"]
            #[inline]
            fn bit_is_clear(&self) -> bool {
                !self.bit()
            }
            #[doc = r" Returns `true` if the bit is set (1)"]
            #[inline]
            fn bit_is_set(&self) -> bool {
                self.bit()
            }

            fn bit(&self) -> bool;
        }
    });

    Ok(code)
}
