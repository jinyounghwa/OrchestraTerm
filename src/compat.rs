#[cfg(test)]
mod tests {
    use vt100::{Color, Parser};

    #[test]
    fn ascii_and_cursor_movement_work() {
        let mut parser = Parser::new(8, 40, 0);
        parser.process(b"hello");
        parser.process(b"\x1b[1D!");
        let screen = parser.screen();
        assert_eq!(
            screen.cell(0, 0).map(|c| c.contents()).as_deref(),
            Some("h")
        );
        assert_eq!(
            screen.cell(0, 3).map(|c| c.contents()).as_deref(),
            Some("l")
        );
        assert_eq!(
            screen.cell(0, 4).map(|c| c.contents()).as_deref(),
            Some("!")
        );
    }

    #[test]
    fn ansi_256_and_truecolor_are_parsed() {
        let mut parser = Parser::new(8, 40, 0);
        parser.process(b"\x1b[38;5;196mR\x1b[48;2;1;2;3mB\x1b[0m");
        let screen = parser.screen();
        let c0 = screen.cell(0, 0).expect("cell 0 must exist");
        let c1 = screen.cell(0, 1).expect("cell 1 must exist");
        assert_eq!(c0.contents(), "R");
        assert_eq!(c0.fgcolor(), Color::Idx(196));
        assert_eq!(c1.contents(), "B");
        assert_eq!(c1.bgcolor(), Color::Rgb(1, 2, 3));
    }

    #[test]
    fn korean_wide_cells_are_tracked() {
        let mut parser = Parser::new(8, 40, 0);
        parser.process("한".as_bytes());
        let screen = parser.screen();
        let c0 = screen.cell(0, 0).expect("first cell must exist");
        let c1 = screen.cell(0, 1).expect("second cell must exist");
        assert_eq!(c0.contents(), "한");
        assert!(!c0.is_wide_continuation());
        assert!(c1.is_wide_continuation());
    }

    #[test]
    fn erase_in_line_sequence_is_applied() {
        let mut parser = Parser::new(8, 40, 0);
        parser.process(b"abcd");
        parser.process(b"\x1b[2D");
        parser.process(b"\x1b[K");
        let screen = parser.screen();
        assert_eq!(
            screen.cell(0, 0).map(|c| c.contents()).as_deref(),
            Some("a")
        );
        assert_eq!(
            screen.cell(0, 1).map(|c| c.contents()).as_deref(),
            Some("b")
        );
        assert_eq!(screen.cell(0, 2).map(|c| c.contents()).as_deref(), Some(""));
        assert_eq!(screen.cell(0, 3).map(|c| c.contents()).as_deref(), Some(""));
    }

    #[test]
    fn alternate_screen_roundtrip_works() {
        let mut parser = Parser::new(8, 40, 100);
        parser.process(b"main");
        parser.process(b"\x1b[?1049h");
        parser.process(b"alt");
        parser.process(b"\x1b[?1049l");
        let screen = parser.screen();
        assert_eq!(
            screen.cell(0, 0).map(|c| c.contents()).as_deref(),
            Some("m")
        );
        assert_eq!(
            screen.cell(0, 1).map(|c| c.contents()).as_deref(),
            Some("a")
        );
        assert_eq!(
            screen.cell(0, 2).map(|c| c.contents()).as_deref(),
            Some("i")
        );
        assert_eq!(
            screen.cell(0, 3).map(|c| c.contents()).as_deref(),
            Some("n")
        );
    }

    #[test]
    fn block_pixel_chars_with_ansi_color_are_preserved() {
        let mut parser = Parser::new(8, 40, 0);
        parser.process(b"\x1b[38;5;46m\xe2\x96\x88\xe2\x96\x88\x1b[0m");
        let screen = parser.screen();
        let c0 = screen.cell(0, 0).expect("cell 0 must exist");
        let c1 = screen.cell(0, 1).expect("cell 1 must exist");
        assert_eq!(c0.contents(), "█");
        assert_eq!(c1.contents(), "█");
        assert_eq!(c0.fgcolor(), Color::Idx(46));
        assert_eq!(c1.fgcolor(), Color::Idx(46));
    }
}
