enum color {
    red = 0xff0000,
    green = 0x00ff00,
    blue = 0x0000ff,
    black = 0x000000,
    white = 0xFFFFFF,
    imaginary = -1,
    purple = 1 << 1,
    orange = 8 >> 1
}

fn main() {
    test_color(red, 0xff0000, ~"red");
    test_color(green, 0x00ff00, ~"green");
    test_color(blue, 0x0000ff, ~"blue");
    test_color(black, 0x000000, ~"black");
    test_color(white, 0xFFFFFF, ~"white");
    test_color(imaginary, -1, ~"imaginary");
    test_color(purple, 2, ~"purple");
    test_color(orange, 4, ~"orange");
}

fn test_color(color: color, val: int, name: ~str) unsafe {
    //assert unsafe::reinterpret_cast(color) == val;
    assert color as int == val;
    assert color as float == val as float;
    assert get_color_alt(color) == name;
    assert get_color_if(color) == name;
}

fn get_color_alt(color: color) -> ~str {
    match color {
      red => {~"red"}
      green => {~"green"}
      blue => {~"blue"}
      black => {~"black"}
      white => {~"white"}
      imaginary => {~"imaginary"}
      purple => {~"purple"}
      orange => {~"orange"}
    }
}

fn get_color_if(color: color) -> ~str {
    if color == red {~"red"}
    else if color == green {~"green"}
    else if color == blue {~"blue"}
    else if color == black {~"black"}
    else if color == white {~"white"}
    else if color == imaginary {~"imaginary"}
    else if color == purple {~"purple"}
    else if color == orange {~"orange"}
    else {~"unknown"}
}


