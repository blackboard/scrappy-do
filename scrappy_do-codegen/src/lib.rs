use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    visit_mut::VisitMut,
    Block, Expr, ExprPath, FnArg, ItemFn, Result, Signature, Token, Type,
};

macro_rules! error {
    ($span:expr, $msg:expr) => {
        syn::Error::new_spanned(&$span, $msg)
    };
    ($span:expr, $($tt:tt)*) => {
        error!($span, format!($($tt)*))
    };
}

mod kw {
    syn::custom_keyword!(item);
    syn::custom_keyword!(context);
}

// Parses `= <value>` in `<name> = <value>` and returns value and span of name-value pair.
fn parse_value(
    input: ParseStream<'_>,
    name: &impl ToTokens,
    has_prev: bool,
) -> Result<(Type, TokenStream)> {
    if input.is_empty() {
        return Err(error!(
            name,
            "expected `{0} = <type>`, found `{0}`",
            name.to_token_stream()
        ));
    }

    let eq_token: Token![=] = input.parse()?;
    if input.is_empty() {
        let span = quote!(#name #eq_token);
        return Err(error!(
            span,
            "expected `{0} = <type>`, found `{0} =`",
            name.to_token_stream()
        ));
    }

    let value: Type = input.parse()?;
    let span = quote!(#name #value);

    if !input.is_empty() {
        let comma = syn::Token![,];
        if input.peek(comma) {
            let _: Token![,] = input.parse()?;
        } else {
            let token = input.parse::<TokenStream>()?;
            return Err(error!(token, "expected `,`, found `{0}`", token));
        }
    }

    if has_prev {
        Err(error!(
            span,
            "duplicate `{}` argument",
            name.to_token_stream()
        ))
    } else {
        Ok((value, span))
    }
}

struct HandleArgs {
    item_ty: Type,
}

struct ConvertYields;

impl VisitMut for ConvertYields {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match expr {
            syn::Expr::Yield(yield_expr) => {
                let value_expr = yield_expr.expr.as_ref().unwrap();
                *expr = syn::parse_quote! {
                    __yield_ind.send((#value_expr).into()).await.expect("live receiver")
                };
            }
            _ => syn::visit_mut::visit_expr_mut(self, expr),
        };
    }
}

impl Parse for HandleArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut item_ty = None;

        while !input.is_empty() {
            if input.peek(kw::item) {
                let i: kw::item = input.parse()?;
                item_ty = Some(parse_value(input, &i, item_ty.is_some())?.0);
            } else {
                let token = input.parse::<TokenStream>()?;
                return Err(error!(token, "unexpected argument: {}", token));
            }
        }

        match item_ty {
            Some(item_ty) => Ok(Self { item_ty }),
            None => {
                let token = input.parse::<TokenStream>()?;
                Err(error!(token, "missing defined item"))
            }
        }
    }
}

/// Attribute to generate a handler function.
///
/// # Required Arguments:
/// - `item`: The struct type the handler scrapes.
///
/// # Example
/// ```ignore
/// #[handle(item = u16)]
/// ```
#[proc_macro_attribute]
pub fn handle(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    proc_macro::TokenStream::from(
        syn::parse(input)
            .map(|ast| {
                impl_handle(TokenStream::from(args), ast).unwrap_or_else(|e| e.to_compile_error())
            })
            .unwrap_or_else(|e| e.to_compile_error()),
    )
}

fn convert_block(block: &mut Block) -> Block {
    ConvertYields.visit_block_mut(block);
    syn::parse2(quote! {
        {
            let (mut __yield_ind, __rec_ind) = scrappy_do::channel(1);
            scrappy_do::spawn(
                async move {
                    #block
                }
            );
            __rec_ind
        }
    })
    .expect("block wrapping")
}

fn convert_fn_signature(sig: Signature, item_ty: Type, context_ty: Type) -> Signature {
    let output = syn::parse2(
        quote!(-> scrappy_do::Receiver<scrappy_do::Indeterminate<#item_ty, #context_ty>>),
    )
    .expect("signature output");
    Signature {
        constness: sig.constness,
        asyncness: sig.asyncness,
        unsafety: sig.unsafety,
        abi: sig.abi,
        fn_token: sig.fn_token,
        ident: sig.ident,
        generics: sig.generics,
        paren_token: sig.paren_token,
        inputs: sig.inputs,
        variadic: sig.variadic,
        output,
    }
}

fn impl_handle(args: TokenStream, ast: ItemFn) -> Result<TokenStream> {
    let HandleArgs { item_ty } = syn::parse2(args)?;
    let context_arg = match ast.sig.inputs.len() {
        // this is a struct method (self + client, context, respone, and logger)
        5 => &ast.sig.inputs[3],
        // this is a bare function
        _ => &ast.sig.inputs[2]
    };
    let context_ty = match &context_arg {
            FnArg::Typed(pat_type) => Ok(pat_type.ty.clone()),
            FnArg::Receiver(arg) => {
                Err(error!(arg, "unexpected argument"))
            }
    }?;

    let mut block = ast.block;
    let block = convert_block(&mut block);
    let signature = convert_fn_signature(ast.sig.clone(), item_ty, *context_ty);

    let new_func = ItemFn {
        attrs: ast.attrs,
        vis: ast.vis,
        sig: signature,
        block: Box::new(block),
    };

    Ok(quote!(#new_func))
}

/// Wraps a function in concrete Handler struct.
///
/// # Example
///
/// Assuming handler function `handler_foo` has been defined by the caller:
/// ```ignore
/// wrap!(function_foo)
/// ```
#[proc_macro]
pub fn wrap(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = syn::parse(input).unwrap();

    impl_wrap(&ast)
}

fn impl_wrap(ast: &ExprPath) -> proc_macro::TokenStream {
    let path = &ast.path;
    let path_name = quote!(#path).to_string();

    let gen = quote! {
        scrappy_do::HandlerImpl::new(#ast, #path_name)
    };
    gen.into()
}
