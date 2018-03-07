// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use syntax::ast::*;
use syntax::codemap::Spanned;
use syntax::ext::base::*;
use syntax::ext::build::AstBuilder;
use syntax::parse::token;
use syntax::print::pprust;
use syntax::tokenstream::{TokenStream, TokenTree};
use syntax_pos::Span;

pub fn expand_assert<'cx>(
    cx: &'cx mut ExtCtxt,
    sp: Span,
    tts: &[TokenTree],
) -> Box<MacResult + 'cx> {
    let mut parser = cx.new_parser_from_tts(tts);
    let cond_expr = panictry!(parser.parse_expr());
    let custom_msg_args = if parser.eat(&token::Comma) {
        let ts = parser.parse_tokens();
        if !ts.is_empty() {
            Some(ts)
        } else {
            None
        }
    } else {
        None
    };

    let sp = sp.with_ctxt(sp.ctxt().apply_mark(cx.current_expansion.mark));
    let panic_call = Mac_ {
        path: Path::from_ident(sp, Ident::from_str("panic")),
        tts: if let Some(ts) = custom_msg_args {
            ts.into()
        } else {
            let panic_str = format!("assertion failed: {}", pprust::expr_to_string(&cond_expr));
            TokenStream::from(token::Literal(
                token::Lit::Str_(Name::intern(&panic_str)),
                None,
            )).into()
        },
    };
    let if_expr = cx.expr_if(
        sp,
        cx.expr(sp, ExprKind::Unary(UnOp::Not, cond_expr)),
        cx.expr(
            sp,
            ExprKind::Mac(Spanned {
                span: sp,
                node: panic_call,
            }),
        ),
        None,
    );
    MacEager::expr(if_expr)
}
