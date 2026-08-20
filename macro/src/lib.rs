use proc_macro2::{
	Delimiter, Group, Ident, Literal, Punct, Spacing, Span, TokenStream, TokenTree,
	token_stream::IntoIter,
};
use quote::{TokenStreamExt, quote, quote_spanned};

/// append to [`TokenStream`] code generating the input tokens.
///
/// **syntax:** `chunk!(stream: ident, ...)`.
///
/// all not interpolated tokens inherit the [`Span::call_site`] span.
///
/// supported syntax:
/// - `#ident`: append the resolved value of `ident` through [`ToTokens`](quote::ToTokens).
/// - `#{expr}`: append the resolved value of `expr` through [`ToTokens`](quote::ToTokens).
/// - `##`: append a `#`.
/// - `#op expr #{tokens}`: append `tokens` based on the evaluation of `op expr`. `op` can be `if`, `for`, `while`, `else`, `match`.
/// - `#do {expr}`: execute `expr` at its difinition point in the strucutre.
/// - other tokens gets appended.
///
/// # example
/// ```
/// let fields = &[
///     (Ident::new("a", Span::call_site()), Ident::new("u32", Span::call_site())),
/// 	(Ident::new("b", Span::call_site()), Ident::new("bool", Span::call_site())),
/// 	(Ident::new("c", Span::call_site()), Ident::new("char", Span::call_site())),
/// 	];
/// let mut stream = TokenStream::new();
/// let public = true;
/// chunk!(stream,
/// 	#if public #{ pub }
/// 	struct Example {
///         #for (field, ty) in fields #{ #field: #ty, }
/// 	}
/// 	impl Example {
///         #do { gen_accessors(stream, fields) }
/// 	}
/// );
/// fn gen_accessors(mut stream: &mut TokenStream, fields: &[(Ident, Ident)]) {
///     chunk!(stream, #for (field, ty) in fields #{
///         fn #{format_ident!("get_{field}")} (&self) -> #ty {
///             self.#field
/// 		}
/// 	});
/// }
/// ```
#[proc_macro]
pub fn chunk(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	chunk_impl(TokenStream::from(input), false).into()
}

/// [`chunk`] but with specified span for all tokens not interpolated.
///
/// **syntax:** `chunk_spanned!(stream: ident, span: expr, ...)`
#[proc_macro]
pub fn chunk_spanned(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	chunk_impl(TokenStream::from(input), true).into()
}

struct Cursor {
	iter: IntoIter,
	cur_token: Option<TokenTree>,
	res: TokenStream,
	end_span: Span,
}

impl Cursor {
	fn new(iter: TokenStream, end_span: Span) -> Self {
		Self { iter: iter.into_iter(), res: TokenStream::new(), cur_token: None, end_span }
	}
	fn next(&mut self) -> Option<TokenTree> {
		self.cur_token.take().or_else(|| self.iter.next())
	}
	fn peek(&mut self) -> Option<&TokenTree> {
		if self.cur_token.is_none() {
			self.cur_token = self.iter.next();
		}
		self.cur_token.as_ref()
	}
	fn peek_kw(&mut self, kw: &str) -> bool {
		match self.peek() {
			Some(TokenTree::Ident(ident)) => ident == kw,
			_ => false,
		}
	}
	fn add<T>(&mut self, tokens: impl IntoIterator<Item = T>)
	where
		TokenStream: Extend<T>,
	{
		self.res.extend(tokens);
	}
	fn expected(&mut self, expected: &str, span: Option<Span>) {
		let span = span.unwrap_or(self.end_span);
		let msg = &format!("expected {expected}");
		self.res.extend(quote_spanned! { span => ::core::compile_error!(#msg); });
	}
	fn eat_ident(&mut self) -> Option<Ident> {
		match self.next() {
			Some(TokenTree::Ident(ident)) => return Some(ident),
			t => self.expected("identifier", t.map(|t| t.span())),
		}
		None
	}
	fn eat_punct(&mut self, char: char) -> bool {
		match self.next() {
			Some(TokenTree::Punct(punct)) if punct.as_char() == char => return true,
			t => self.expected(&format!("`{char}`"), t.map(|t| t.span())),
		}
		false
	}
	fn eat_brace(&mut self) -> Option<Group> {
		match self.next() {
			Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => return Some(g),
			t => self.expected("`(`", t.map(|t| t.span())),
		}
		None
	}
	fn eat_until(
		&mut self, expected: &str, pred: impl Fn(&TokenTree) -> bool,
	) -> Option<TokenStream> {
		let mut tokens = TokenStream::new();
		while let Some(token) = self.peek() {
			if pred(token) {
				break;
			}
			tokens.append(self.next().unwrap());
		}
		if tokens.is_empty() {
			let span = self.peek().map(|t| t.span());
			self.expected(expected, span);
			return None;
		}
		Some(tokens)
	}
}

fn is_punct(token: &TokenTree, char: char) -> bool {
	matches!(token, TokenTree::Punct(punct) if punct.as_char() == char)
}

fn chunk_impl(input: TokenStream, spanned: bool) -> TokenStream {
	let mut cur = Cursor::new(input, Span::call_site());

	let Some(stream_ident) = cur.eat_ident() else { return cur.res };
	if !cur.eat_punct(',') {
		return cur.res;
	}

	if spanned {
		let Some(span) = cur.eat_until("an expression", |t| is_punct(t, ',')) else {
			return cur.res;
		};
		if !cur.eat_punct(',') {
			return cur.res;
		}
		cur.add(quote! { let __span = #span; });
	} else {
		cur.add(quote! { let __span = ::chunked_quote::__private::Span::call_site(); })
	}

	quote_stream(&mut cur, &stream_ident);
	let res = cur.res;
	quote! { #[allow(unused_braces)] { #res } }
}

fn quote_punct(cur: &mut Cursor, punct: &Punct, stream_ident: &Ident) {
	let spacing = match punct.spacing() {
		Spacing::Joint => quote! { ::chunked_quote::__private::Spacing::Joint },
		Spacing::Alone => quote! { ::chunked_quote::__private::Spacing::Alone },
	};
	let ch = punct.as_char();
	cur.add(quote! {{
		let mut __punct = ::chunked_quote::__private::Punct::new(#ch, #spacing);
		__punct.set_span(__span);
		#stream_ident.extend(Some(__punct));
	}});
}

fn quote_ident(cur: &mut Cursor, ident: &Ident, stream_ident: &Ident) {
	let name = ident.to_string();
	match name.strip_prefix("r#") {
		Some(name) => cur.add(quote! {
			#stream_ident.extend(Some(::chunked_quote::__private::Ident::new_raw(#name, __span)));
		}),
		None => cur.add(quote! {
			#stream_ident.extend(Some(::chunked_quote::__private::Ident::new(#name, __span)));
		}),
	}
}

fn quote_literal(cur: &mut Cursor, lit: &Literal, stream_ident: &Ident) {
	let text = lit.to_string();
	cur.add(quote! {{
		let mut __lit = <::chunked_quote::__private::Literal as ::core::str::FromStr>
			::from_str(#text).unwrap();
		__lit.set_span(__span);
		#stream_ident.extend(Some(__lit));
	}});
}

fn quote_group(cur: &mut Cursor, group: &Group, stream_ident: &Ident) {
	let delimiter = match group.delimiter() {
		Delimiter::Parenthesis => quote! { ::chunked_quote::__private::Delimiter::Parenthesis },
		Delimiter::Brace => quote! { ::chunked_quote::__private::Delimiter::Brace },
		Delimiter::Bracket => quote! { ::chunked_quote::__private::Delimiter::Bracket },
		Delimiter::None => quote! { ::chunked_quote::__private::Delimiter::None },
	};
	let mut inner_cur = Cursor::new(group.stream(), group.span_close());
	quote_stream(&mut inner_cur, stream_ident);
	let inner = inner_cur.res;
	cur.add(quote! { #stream_ident.extend(Some({
		let mut #stream_ident = ::chunked_quote::__private::TokenStream::new();
		#inner
		let mut __group = ::chunked_quote::__private::Group::new(#delimiter, #stream_ident);
		__group.set_span(__span);
		__group
	})); })
}

fn quote_stream(cur: &mut Cursor, stream_ident: &Ident) {
	while let Some(token) = cur.next() {
		match token {
			TokenTree::Ident(ident) => quote_ident(cur, &ident, stream_ident),
			TokenTree::Literal(lit) => quote_literal(cur, &lit, stream_ident),
			TokenTree::Punct(p) if p.as_char() == '#' => {
				handle_directive(cur, stream_ident);
			}
			TokenTree::Punct(punct) => quote_punct(cur, &punct, stream_ident),
			TokenTree::Group(group) => quote_group(cur, &group, stream_ident),
		}
	}
}

fn kw_expr_body(cur: &mut Cursor, ident: Ident, stream_ident: &Ident) -> Option<()> {
	cur.add(Some(ident));
	let expr = cur.eat_until("an expression", |t| is_punct(t, '#'))?;
	cur.add(expr);
	body(cur, true, stream_ident)
}
fn body(cur: &mut Cursor, eat_hash: bool, stream_ident: &Ident) -> Option<()> {
	if eat_hash {
		cur.eat_punct('#').then_some(())?;
	}
	let body = cur.eat_brace()?;
	let mut inner_cur = Cursor::new(body.stream(), body.span_close());
	quote_stream(&mut inner_cur, stream_ident);
	cur.add(Some(Group::new(Delimiter::Brace, inner_cur.res)));
	Some(())
}

fn is_brace(token: &TokenTree) -> bool {
	matches!(token, TokenTree::Group(group) if group.delimiter() == Delimiter::Brace)
}

fn handle_match(cur: &mut Cursor, ident: Ident, stream_ident: &Ident) -> Option<()> {
	cur.add(Some(ident));
	let expr = cur.eat_until("an expression", is_brace)?;
	cur.add(expr);
	let arms = cur.eat_brace()?;
	let mut arms_cur = Cursor::new(arms.stream(), arms.span_close());
	while let Some(token) = arms_cur.next() {
		if is_punct(&token, '#') {
			body(&mut arms_cur, false, stream_ident)?;
		} else {
			arms_cur.add(Some(token));
		}
	}
	cur.add(Some(Group::new(Delimiter::Brace, arms_cur.res)));
	Some(())
}

fn handle_directive(cur: &mut Cursor, stream_ident: &Ident) -> Option<()> {
	match cur.next() {
		Some(TokenTree::Punct(p)) if p.as_char() == '#' => quote_punct(cur, &p, stream_ident),
		Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace => cur.add(quote! {
			::chunked_quote::__private::ToTokens::to_tokens(&#group, &mut #stream_ident);
		}),
		Some(TokenTree::Ident(ident)) => match &*ident.to_string() {
			"if" | "for" | "while" => kw_expr_body(cur, ident, stream_ident)?,
			"else" => {
				if cur.peek_kw("if") {
					kw_expr_body(cur, ident, stream_ident)?
				} else {
					cur.add(Some(ident));
					body(cur, true, stream_ident)?
				}
			}
			"do" => {
				let block = cur.eat_brace()?;
				cur.add(quote! {{ let #stream_ident = &mut #stream_ident; #block }})
			}
			"match" => handle_match(cur, ident, stream_ident)?,
			_ => cur.add(quote! {
				::chunked_quote::__private::ToTokens::to_tokens(&#ident, &mut #stream_ident);
			}),
		},
		t => cur.expected("an identifier, `#` or `{`", t.map(|t| t.span())),
	};
	Some(())
}
