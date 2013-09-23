// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[allow(cstack)]; // each rendering task runs on a fixed stack segment.

use std::fmt;
use std::libc;
use std::rt::io;
use std::vec;

pub struct Markdown<'self>(&'self str);

static OUTPUT_UNIT: libc::size_t = 64;

type sd_markdown = libc::c_void;  // this is opaque to us

// this is a large struct of callbacks we don't use
type sd_callbacks = [libc::size_t, ..26];

struct html_toc_data {
    header_count: libc::c_int,
    current_level: libc::c_int,
    level_offset: libc::c_int,
}

struct html_renderopt {
    toc_data: html_toc_data,
    flags: libc::c_uint,
    link_attributes: Option<extern "C" fn(*buf, *buf, *libc::c_void)>,
}

struct buf {
    data: *u8,
    size: libc::size_t,
    asize: libc::size_t,
    unit: libc::size_t,
}

// sundown FFI
extern {
    fn sdhtml_renderer(callbacks: *sd_callbacks,
                       options_ptr: *html_renderopt,
                       render_flags: libc::c_uint);
    fn sd_markdown_new(extensions: libc::c_uint,
                       max_nesting: libc::size_t,
                       callbacks: *sd_callbacks,
                       opaque: *libc::c_void) -> *sd_markdown;
    fn sd_markdown_render(ob: *buf,
                          document: *u8,
                          doc_size: libc::size_t,
                          md: *sd_markdown);
    fn sd_markdown_free(md: *sd_markdown);

    fn bufnew(unit: libc::size_t) -> *buf;
    fn bufrelease(b: *buf);

}

fn render(w: &mut io::Writer, s: &str) {
    // This code is all lifted from examples/sundown.c in the sundown repo
    unsafe {
        let ob = bufnew(OUTPUT_UNIT);
        let options = html_renderopt {
            toc_data: html_toc_data {
                header_count: 0,
                current_level: 0,
                level_offset: 0,
            },
            flags: 0,
            link_attributes: None,
        };
        let callbacks: sd_callbacks = [0, ..26];

        sdhtml_renderer(&callbacks, &options, 0);
        let markdown = sd_markdown_new(0, 16, &callbacks,
                                       &options as *html_renderopt as *libc::c_void);

        do s.as_imm_buf |data, len| {
            sd_markdown_render(ob, data, len as libc::size_t, markdown);
        }
        sd_markdown_free(markdown);

        do vec::raw::buf_as_slice((*ob).data, (*ob).size as uint) |buf| {
            w.write(buf);
        }

        bufrelease(ob);
    }
}

impl<'self> fmt::Default for Markdown<'self> {
    fn fmt(md: &Markdown<'self>, fmt: &mut fmt::Formatter) {
        // This is actually common enough to special-case
        if md.len() == 0 { return; }

        render(fmt.buf, md.as_slice());
    }
}
