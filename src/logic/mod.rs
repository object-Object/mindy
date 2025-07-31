pub mod ast;
mod vm;

lalrpop_util::lalrpop_mod!(grammar, "/logic/grammar.rs");

pub use grammar::LogicParser;
pub use vm::*;

#[cfg(test)]
mod tests {
    use super::{ast::*, *};
    use crate::types::colors::{COLORS, rgba8888_to_double_bits, to_double_bits};

    macro_rules! assert_ast {
        ($input:expr, $($x:expr),* $(,)?) => {
            assert_eq!(
                LogicParser::new().parse($input).unwrap(),
                vec![$($x),*],
                "{}",
                $input,
            )
        };
    }

    macro_rules! instruction {
        ($name:expr) => {
            instruction!($name,)
        };
        ($name:expr, $($x:expr),* $(,)?) => {
            Statement::Instruction($name.into(), vec![$($x),*])
        };
    }

    fn variable(value: &str) -> Value {
        Value::Variable(value.into())
    }

    fn string(value: &str) -> Value {
        Value::String(value.into())
    }

    fn number<T>(value: T) -> Value
    where
        T: Into<f64>,
    {
        Value::Number(value.into())
    }

    #[test]
    fn test_empty() {
        assert_ast!["",];
    }

    #[test]
    fn test_comment_only() {
        assert_ast!["# foo",];
    }

    #[test]
    fn test_comments_only() {
        assert_ast![
            "
            # foo
            # bar
            ",
        ];
    }

    #[test]
    fn test_noop() {
        assert_ast!["noop", instruction!("noop")];
    }

    #[test]
    fn test_semicolon() {
        assert_ast![
            "noop;noop ; noop",
            instruction!("noop"),
            instruction!("noop"),
            instruction!("noop"),
        ];
    }

    #[test]
    fn test_string() {
        assert_ast![
            r##"
            print "foo"
            print "a\nb"
            print "#"
            print "\"
            "##,
            instruction!("print", string("foo")),
            instruction!("print", string("a\nb")),
            instruction!("print", string("#")),
            instruction!("print", string(r"\")),
        ];
    }

    #[test]
    fn test_number() {
        for (input, value) in vec![
            ("0", number(0)),
            ("0.0", number(0)),
            ("--0.-0.", number(0)),
            ("--.-0.", variable("--.-0.")),
            ("1", number(1)),
            ("++1f", number(1)),
            ("--1", number(-1)),
            ("1.5F", number(1.5)),
            ("1.5FF", variable("1.5FF")),
            ("-1.5", number(-1.5)),
            ("0b101", number(5)),
            ("-0b1111", number(-15)),
            ("0xDeadbeeF", number(0xdeadbeefu32)),
            ("-0x123abc", number(-0x123abc)),
            ("%[red]", number(COLORS["red"])),
            ("%[GREEN]", number(COLORS["GREEN"])),
            ("%[foo]", variable("%[foo]")),
            ("%DeadbeeF", number(rgba8888_to_double_bits(0xdeadbeef))),
            ("%-1+2-3+4", number(to_double_bits(-1, 2, -3, 4))),
            ("%-f-f-f-f", number(rgba8888_to_double_bits(0xfffffff1))),
            ("%123aBc", number(rgba8888_to_double_bits(0x123abcff))),
            ("%+A-b+c", number(to_double_bits(0xa, -0xb, 0xc, 0xff))),
        ] {
            let input = format!("print {input}");
            println!("{input}");
            assert_ast!(&input, instruction!("print", value));
        }
    }
}
