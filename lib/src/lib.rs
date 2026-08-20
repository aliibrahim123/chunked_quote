//! [`quote`] using only one [`TokenStream`](proc_macro2::TokenStream).
//!
//! this crate provide a [`quote`] inspired macro [`chunk!`] that builds [`TokenStream`](proc_macro2::TokenStream)s though direct appending not object composition.
//!
//! it support all [`quote`] features except `macro_rules` inspired repetetion, and add additional features like control flow and do blocks.

pub use chunked_quote_impl::{chunk, chunk_spanned, quote, quote_spanned};

#[doc(hidden)]
pub mod __private {
	pub use proc_macro2::{
		Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree,
	};
	pub use quote::ToTokens;
}
