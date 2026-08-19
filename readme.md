# chunked quote
just a premature optimization for [`quote!`](https://docs.rs/quote/) where instead of object composition with multiple `TokenStream`, you use a single `TokenStream` passed to all `chunk!` macro invocations.

this is a prototype and not production ready.