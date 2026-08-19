pub use chunked_quote_impl::chunk;

#[doc(hidden)]
pub mod __private {
    pub use proc_macro2::{
        Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree,
    };
    pub use quote::ToTokens;
}
