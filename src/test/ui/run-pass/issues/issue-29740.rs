// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// run-pass
// Regression test for #29740. Inefficient MIR matching algorithms
// generated way too much code for this sort of case, leading to OOM.
#![allow(non_snake_case)]

pub mod KeyboardEventConstants {
    pub const DOM_KEY_LOCATION_STANDARD: u32 = 0;
    pub const DOM_KEY_LOCATION_LEFT: u32 = 1;
    pub const DOM_KEY_LOCATION_RIGHT: u32 = 2;
    pub const DOM_KEY_LOCATION_NUMPAD: u32 = 3;
} // mod KeyboardEventConstants

pub enum Key {
    Space,
    Apostrophe,
    Comma,
    Minus,
    Period,
    Slash,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Semicolon,
    Equal,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    LeftBracket,
    Backslash,
    RightBracket,
    GraveAccent,
    World1,
    World2,

    Escape,
    Enter,
    Tab,
    Backspace,
    Insert,
    Delete,
    Right,
    Left,
    Down,
    Up,
    PageUp,
    PageDown,
    Home,
    End,
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    Kp0,
    Kp1,
    Kp2,
    Kp3,
    Kp4,
    Kp5,
    Kp6,
    Kp7,
    Kp8,
    Kp9,
    KpDecimal,
    KpDivide,
    KpMultiply,
    KpSubtract,
    KpAdd,
    KpEnter,
    KpEqual,
    LeftShift,
    LeftControl,
    LeftAlt,
    LeftSuper,
    RightShift,
    RightControl,
    RightAlt,
    RightSuper,
    Menu,
}

fn key_from_string(key_string: &str, location: u32) -> Option<Key> {
    match key_string {
        " " => Some(Key::Space),
        "\"" => Some(Key::Apostrophe),
        "'" => Some(Key::Apostrophe),
        "<" => Some(Key::Comma),
        "," => Some(Key::Comma),
        "_" => Some(Key::Minus),
        "-" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Minus),
        ">" => Some(Key::Period),
        "." if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Period),
        "?" => Some(Key::Slash),
        "/" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Slash),
        "~" => Some(Key::GraveAccent),
        "`" => Some(Key::GraveAccent),
        ")" => Some(Key::Num0),
        "0" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num0),
        "!" => Some(Key::Num1),
        "1" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num1),
        "@" => Some(Key::Num2),
        "2" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num2),
        "#" => Some(Key::Num3),
        "3" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num3),
        "$" => Some(Key::Num4),
        "4" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num4),
        "%" => Some(Key::Num5),
        "5" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num5),
        "^" => Some(Key::Num6),
        "6" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num6),
        "&" => Some(Key::Num7),
        "7" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num7),
        "*" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num8),
        "8" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num8),
        "(" => Some(Key::Num9),
        "9" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Num9),
        ":" => Some(Key::Semicolon),
        ";" => Some(Key::Semicolon),
        "+" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Equal),
        "=" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD => Some(Key::Equal),
        "A" => Some(Key::A),
        "a" => Some(Key::A),
        "B" => Some(Key::B),
        "b" => Some(Key::B),
        "C" => Some(Key::C),
        "c" => Some(Key::C),
        "D" => Some(Key::D),
        "d" => Some(Key::D),
        "E" => Some(Key::E),
        "e" => Some(Key::E),
        "F" => Some(Key::F),
        "f" => Some(Key::F),
        "G" => Some(Key::G),
        "g" => Some(Key::G),
        "H" => Some(Key::H),
        "h" => Some(Key::H),
        "I" => Some(Key::I),
        "i" => Some(Key::I),
        "J" => Some(Key::J),
        "j" => Some(Key::J),
        "K" => Some(Key::K),
        "k" => Some(Key::K),
        "L" => Some(Key::L),
        "l" => Some(Key::L),
        "M" => Some(Key::M),
        "m" => Some(Key::M),
        "N" => Some(Key::N),
        "n" => Some(Key::N),
        "O" => Some(Key::O),
        "o" => Some(Key::O),
        "P" => Some(Key::P),
        "p" => Some(Key::P),
        "Q" => Some(Key::Q),
        "q" => Some(Key::Q),
        "R" => Some(Key::R),
        "r" => Some(Key::R),
        "S" => Some(Key::S),
        "s" => Some(Key::S),
        "T" => Some(Key::T),
        "t" => Some(Key::T),
        "U" => Some(Key::U),
        "u" => Some(Key::U),
        "V" => Some(Key::V),
        "v" => Some(Key::V),
        "W" => Some(Key::W),
        "w" => Some(Key::W),
        "X" => Some(Key::X),
        "x" => Some(Key::X),
        "Y" => Some(Key::Y),
        "y" => Some(Key::Y),
        "Z" => Some(Key::Z),
        "z" => Some(Key::Z),
        "{" => Some(Key::LeftBracket),
        "[" => Some(Key::LeftBracket),
        "|" => Some(Key::Backslash),
        "\\" => Some(Key::Backslash),
        "}" => Some(Key::RightBracket),
        "]" => Some(Key::RightBracket),
        "Escape" => Some(Key::Escape),
        "Enter" if location == KeyboardEventConstants::DOM_KEY_LOCATION_STANDARD
                => Some(Key::Enter),
        "Tab" => Some(Key::Tab),
        "Backspace" => Some(Key::Backspace),
        "Insert" => Some(Key::Insert),
        "Delete" => Some(Key::Delete),
        "ArrowRight" => Some(Key::Right),
        "ArrowLeft" => Some(Key::Left),
        "ArrowDown" => Some(Key::Down),
        "ArrowUp" => Some(Key::Up),
        "PageUp" => Some(Key::PageUp),
        "PageDown" => Some(Key::PageDown),
        "Home" => Some(Key::Home),
        "End" => Some(Key::End),
        "CapsLock" => Some(Key::CapsLock),
        "ScrollLock" => Some(Key::ScrollLock),
        "NumLock" => Some(Key::NumLock),
        "PrintScreen" => Some(Key::PrintScreen),
        "Pause" => Some(Key::Pause),
        "F1" => Some(Key::F1),
        "F2" => Some(Key::F2),
        "F3" => Some(Key::F3),
        "F4" => Some(Key::F4),
        "F5" => Some(Key::F5),
        "F6" => Some(Key::F6),
        "F7" => Some(Key::F7),
        "F8" => Some(Key::F8),
        "F9" => Some(Key::F9),
        "F10" => Some(Key::F10),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
        "F13" => Some(Key::F13),
        "F14" => Some(Key::F14),
        "F15" => Some(Key::F15),
        "F16" => Some(Key::F16),
        "F17" => Some(Key::F17),
        "F18" => Some(Key::F18),
        "F19" => Some(Key::F19),
        "F20" => Some(Key::F20),
        "F21" => Some(Key::F21),
        "F22" => Some(Key::F22),
        "F23" => Some(Key::F23),
        "F24" => Some(Key::F24),
        "F25" => Some(Key::F25),
        "0" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp0),
        "1" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp1),
        "2" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp2),
        "3" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp3),
        "4" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp4),
        "5" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp5),
        "6" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp6),
        "7" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp7),
        "8" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp8),
        "9" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::Kp9),
        "." if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::KpDecimal),
        "/" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::KpDivide),
        "*" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::KpMultiply),
        "-" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::KpSubtract),
        "+" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::KpAdd),
        "Enter" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD
                => Some(Key::KpEnter),
        "=" if location == KeyboardEventConstants::DOM_KEY_LOCATION_NUMPAD => Some(Key::KpEqual),
        "Shift" if location == KeyboardEventConstants::DOM_KEY_LOCATION_LEFT
                => Some(Key::LeftShift),
        "Control" if location == KeyboardEventConstants::DOM_KEY_LOCATION_LEFT
                => Some(Key::LeftControl),
        "Alt" if location == KeyboardEventConstants::DOM_KEY_LOCATION_LEFT => Some(Key::LeftAlt),
        "Super" if location == KeyboardEventConstants::DOM_KEY_LOCATION_LEFT
                => Some(Key::LeftSuper),
        "Shift" if location == KeyboardEventConstants::DOM_KEY_LOCATION_RIGHT
                => Some(Key::RightShift),
        "Control" if location == KeyboardEventConstants::DOM_KEY_LOCATION_RIGHT
                => Some(Key::RightControl),
        "Alt" if location == KeyboardEventConstants::DOM_KEY_LOCATION_RIGHT => Some(Key::RightAlt),
        "Super" if location == KeyboardEventConstants::DOM_KEY_LOCATION_RIGHT
                => Some(Key::RightSuper),
        "ContextMenu" => Some(Key::Menu),
        _ => None
    }
}

fn main() { }
