use heck::ToUpperCamelCase as _;
use kinetics_parser::{Cron, Endpoint, Worker};
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    fold::{fold_expr, Fold},
    parse_macro_input, Block, Expr, ExprMethodCall, ExprPath, FnArg, Ident, ItemFn, Lit, Pat,
};

/// Lambda endpoint
///
/// Parameters:
/// - `name`: override the function name
/// - `url_path`: URL path of the endpoint
/// - `environment`: environment variables
/// - `queues`: SQS queues accessible from the lambda
///
/// The macro expands a function into a version that
/// automatically generates a `PathPattern` implementation and
/// rewrites `event.path_param("...")` calls,
/// so that they correctly work with the generated PathPattern.
#[proc_macro_attribute]
pub fn endpoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.
    let args = parse_macro_input!(attr as Endpoint);

    // Parse the function
    let mut func = parse_macro_input!(item as ItemFn);
    let fn_name = func.sig.ident.to_string();
    let first_arg = func
        .sig
        .inputs
        .first()
        .expect("function must have at least one argument");
    let event_ident = match first_arg {
        FnArg::Typed(pat_type) => match &*pat_type.pat {
            Pat::Ident(pat_ident) => pat_ident.ident.clone(),
            _ => panic!("expected a simple identifier for the first argument"),
        },
        _ => panic!("expected a typed argument"),
    };

    // Build the FuncitionNamePath struct used for path parsing
    let struct_name = format!("{}Path", fn_name.to_upper_camel_case());
    let url_path = args.url_path.unwrap_or(fn_name);
    let struct_ident = Ident::new(&struct_name, func.sig.ident.span());
    let struct_item: syn::Block = syn::parse2(quote! {{
        pub struct #struct_ident;
        impl kinetics::tools::http::PathPattern for #struct_ident {
            const PATTERN: &'static str = #url_path;
        }
    }})
    .expect("Invalid struct_item");

    // Walk the function body and replace `event.path_param("…")`
    struct ReplacePathParam<'a> {
        event_ident: &'a Ident,
        path_struct_ident: &'a Ident,
    }

    impl<'a> Fold for ReplacePathParam<'a> {
        fn fold_expr(&mut self, expr: Expr) -> Expr {
            // Is it `event.path_param(...)` ?
            if let Expr::MethodCall(ExprMethodCall {
                receiver,
                method,
                args,
                ..
            }) = expr.clone()
            {
                if let Expr::Path(ExprPath { path, .. }) = *receiver {
                    if let Some(ident) = path.get_ident() {
                        if ident == self.event_ident && method == "path_param" {
                            // Grab the first argument – it must be a string literal
                            if let Some(Expr::Lit(ref lit_expr)) = args.first() {
                                if let Lit::Str(ref lit_str) = lit_expr.lit {
                                    // Build the replacement expression
                                    // <http::Request<Body> as PathExt<EndpointPath>>::path_param(&event, "name");
                                    let ReplacePathParam {
                                        event_ident,
                                        path_struct_ident,
                                    } = self;
                                    let new_expr: Expr = syn::parse2(quote! {
                                        #event_ident.path_param::<#path_struct_ident>(
                                            #lit_str
                                        )
                                    })
                                    .expect("failed to build replacement expression");
                                    return new_expr;
                                }
                            }
                        }
                    }
                }
            }
            // Default – recurse into sub‑expressions
            fold_expr(self, expr)
        }
    }

    let mut replacer = ReplacePathParam {
        event_ident: &event_ident,
        path_struct_ident: &struct_ident,
    };

    let folded_block = replacer.fold_block(*func.block);

    // Assemble the new block: struct + folded body
    func.block = Box::new(Block {
        brace_token: folded_block.brace_token,
        stmts: [struct_item.stmts, folded_block.stmts].concat(),
    });
    TokenStream::from(quote! { #func })
}

/// Cron lambda
///
/// Parameters:
/// - `name`: override the function name
/// - `schedule`: [Schedule expression](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/aws-resource-scheduler-schedule.html#cfn-scheduler-schedule-scheduleexpression)
/// - `environment`: environment variables
#[proc_macro_attribute]
pub fn cron(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.
    let _args = parse_macro_input!(attr as Cron);
    item
}

/// Worker lambda
///
/// Parameters:
/// - `name`: override the function name
/// - `concurrency`: max number of concurrent workers
/// - `fifo`: set to true to enable FIFO processing
/// - `environment`: environment variables
#[proc_macro_attribute]
pub fn worker(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the macro attributes in order to validate the inputs,
    // then discard the result.s
    let _args = parse_macro_input!(attr as Worker);
    item
}
