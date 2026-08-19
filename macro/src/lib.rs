use proc_macro2::{
	Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree, token_stream::IntoIter,
};
use quote::{TokenStreamExt, quote, quote_spanned};

#[proc_macro]
pub fn chunk(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input = TokenStream::from(input).into_iter();
	let mut result = TokenStream::new();
	chunk_impl(input, &mut result, false);
	result.into()
}

#[proc_macro]
pub fn chunk_spanned(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let input = TokenStream::from(input).into_iter();
	let mut result = TokenStream::new();
	chunk_impl(input, &mut result, true);
	result.into()
}

fn chunk_impl(mut input: IntoIter, result: &mut TokenStream, spanned: bool) {
	let stream_ident = match input.next() {
		Some(TokenTree::Ident(ident)) => ident,
		Some(token) => return error(result, token.span(), "expected an identifier"),
		_ => return error(result, Span::call_site(), "expected an identifier"),
	};
	match input.next() {
		Some(TokenTree::Punct(punct)) if punct.as_char() == ',' => {}
		Some(token) => return error(result, token.span(), "expected `,`"),
		_ => return error(result, Span::call_site(), "expected `,`"),
	};
	let span = if spanned {
		let mut span = TokenStream::new();
		loop {
			let Some(token) = input.next() else {
				return error(result, Span::call_site(), "expected `,`");
			};
			if matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ',') {
				break;
			}
			span.append(token);
		}
		if span.is_empty() {
			return error(result, Span::call_site(), "expected an expression");
		}
		span
	} else {
		quote! { ::chunked_quote::__private::Span::call_site() }
	};
	let stream = quote_stream(input, &stream_ident, Span::call_site());
	result.extend(quote! { #[allow(unused_braces)] { let __span = #span; #stream #stream_ident } });
}

fn emit_punct(punct: &Punct, stream_ident: &Ident, result: &mut TokenStream) {
	let spacing = match punct.spacing() {
		Spacing::Joint => quote! { ::chunked_quote::__private::Spacing::Joint },
		Spacing::Alone => quote! { ::chunked_quote::__private::Spacing::Alone },
	};
	let punct = punct.as_char();
	result.extend(quote! {
		let mut __punct = ::chunked_quote::__private::Punct::new(#punct, #spacing);
		__punct.set_span(__span);
		#stream_ident.extend(Some(__punct));
	})
}

fn quote_stream(mut stream: IntoIter, stream_ident: &Ident, end_span: Span) -> TokenStream {
	let mut result = TokenStream::new();
	while let Some(token) = stream.next() {
		match token {
			TokenTree::Ident(ident) => {
				let ident = ident.to_string();
				let ident = match ident.strip_prefix("r#") {
					Some(ident) => {
						quote! { ::chunked_quote::__private::Ident::new_raw(#ident, __span) }
					}
					None => quote! { ::chunked_quote::__private::Ident::new(#ident, __span) },
				};
				result.extend(quote! { #stream_ident.extend(Some(#ident)); })
			}
			TokenTree::Literal(lit) => {
				let lit = lit.to_string();
				result.extend(quote! {
					let mut __lit = <::chunked_quote::__private::Literal as ::core::str::FromStr>
						::from_str(#lit).unwrap();
					__lit.set_span(__span);
					#stream_ident.extend(Some(__lit));
				})
			}
			TokenTree::Punct(punct) => {
				if punct.as_char() == '#' {
					handle_directive(&mut stream, stream_ident, end_span, &mut result);
				} else {
					emit_punct(&punct, stream_ident, &mut result)
				}
			}
			TokenTree::Group(group) => {
				let delimiter = match group.delimiter() {
					Delimiter::Parenthesis => {
						quote! { ::chunked_quote::__private::Delimiter::Parenthesis }
					}
					Delimiter::Brace => quote! { ::chunked_quote::__private::Delimiter::Brace },
					Delimiter::Bracket => quote! { ::chunked_quote::__private::Delimiter::Bracket },
					Delimiter::None => quote! { ::chunked_quote::__private::Delimiter::None },
				};
				let stream =
					quote_stream(group.stream().into_iter(), stream_ident, group.span_close());
				result.extend(quote! {
					let __group = {
						let mut #stream_ident = ::chunked_quote::__private::TokenStream::new();
						#stream
						let mut __group = ::chunked_quote::__private::Group::new(#delimiter, #stream_ident);
						__group.set_span(__span);
						__group
					};
					#stream_ident.extend(Some(__group));
				})
			}
		}
	}
	result
}

fn error(result: &mut TokenStream, span: Span, expected: &str) {
	result.extend(quote_spanned! {
		span => ::core::compile_error!(#expected);
	})
}

fn eat_expr_and_block(
	stream: &mut IntoIter, result: &mut TokenStream, end_span: Span,
) -> Option<(Vec<TokenTree>, Group)> {
	let mut expr = Vec::new();
	while let Some(token) = stream.next() {
		if let TokenTree::Group(group) = &token
			&& group.delimiter() == Delimiter::Brace
		{
			return if expr.is_empty() {
				error(result, group.span_open(), "expected an expression");
				None
			} else {
				let TokenTree::Group(group) = token else { unreachable!() };
				Some((expr, group))
			};
		}
		expr.push(token);
	}
	error(result, end_span, "expected `{`");
	None
}

fn handle_match(
	stream: &mut IntoIter, stream_ident: &Ident, match_span: Span, end_span: Span,
	result: &mut TokenStream,
) {
	let Some((expr, block)) = eat_expr_and_block(stream, result, end_span) else {
		return;
	};
	let mut stream = block.stream().into_iter();
	let mut arms = TokenStream::new();
	let mut pat = Vec::new();
	while let Some(token) = stream.next() {
		if let TokenTree::Group(group) = &token
			&& group.delimiter() == Delimiter::Brace
		{
			let stream = quote_stream(group.stream().into_iter(), stream_ident, group.span_close());
			arms.extend(pat);
			arms.extend(Some(Group::new(Delimiter::Brace, stream)));
			pat = Vec::new();
		} else {
			pat.push(token);
		}
	}
	arms.extend(pat);
	let match_ident = Ident::new("match", match_span);
	result.extend(quote! { #match_ident #(#expr)* { #arms }; });
}

fn handle_directive(
	stream: &mut IntoIter, stream_ident: &Ident, end_span: Span, result: &mut TokenStream,
) {
	match stream.next() {
		Some(TokenTree::Punct(punct)) if punct.as_char() == '#' => {
			emit_punct(&punct, stream_ident, result)
		}
		Some(TokenTree::Ident(ident)) => match &*ident.to_string() {
			"if" | "else" | "for" | "while" | "loop" => {
				let Some((expr, block)) = eat_expr_and_block(stream, result, end_span) else {
					return;
				};
				let stream =
					quote_stream(block.stream().into_iter(), stream_ident, block.span_close());
				result.extend(quote! { #ident #(#expr)* { #stream } })
			}
			"match" => handle_match(stream, stream_ident, ident.span(), end_span, result),
			"do" => {
				let group = match stream.next() {
					Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace => group,
					Some(token) => return error(result, token.span(), "expected `{`"),
					_ => return error(result, end_span, "expected `{`"),
				};
				result.extend(quote! {{ let #stream_ident = &mut #stream_ident; #group }});
			}
			_ => result.extend(quote! {
				::chunked_quote::__private::ToTokens::to_tokens(&#ident, &mut #stream_ident);
			}),
		},
		Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace => {
			result.extend(quote! {
				::chunked_quote::__private::ToTokens::to_tokens(&#group, &mut #stream_ident);
			})
		}
		Some(token) => error(result, token.span(), "expected identifier, `#` or `{`"),
		_ => error(result, end_span, "expected identifier, `#` or `{`"),
	}
}
