pub mod ast;
pub mod vm;

lalrpop_util::lalrpop_mod!(
    #[allow(deprecated)]
    grammar,
    "/logic/grammar.rs"
);

pub use grammar::LogicParser;

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::{
        ast::{Instruction::*, *},
        *,
    };
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
        ($i:expr) => {
            instruction!($i,)
        };
        ($i:expr, $($x:expr),* $(,)?) => {
            Statement::Instruction($i, vec![$($x),*])
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

    // general syntax

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
    fn test_semicolon() {
        assert_ast![
            "noop;noop ; noop",
            instruction!(Noop),
            instruction!(Noop),
            instruction!(Noop),
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
            instruction!(Print {
                value: string("foo")
            }),
            instruction!(Print {
                value: string("a\nb")
            }),
            instruction!(Print { value: string("#") }),
            instruction!(Print {
                value: string(r"\")
            }),
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
            ("+-1", number(-1)),
            ("-+1", number(-1)),
            ("--1", number(1)),
            ("1.5F", number(1.5)),
            ("1.5FF", variable("1.5FF")),
            ("-1", number(-1)),
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
            assert_ast![&input, instruction!(Print { value })];
        }
    }

    #[test]
    fn test_extra() {
        assert_ast![
            "
            print foo bar baz
            print 1 2
            noop a
            ",
            instruction!(
                Print {
                    value: variable("foo")
                },
                variable("bar"),
                variable("baz"),
            ),
            instruction!(Print { value: number(1) }, number(2),),
            instruction!(Noop, variable("a")),
        ];
    }

    #[test]
    fn test_unknown() {
        assert_ast![
            "
            foo
            bar baz 1
            ",
            instruction!(Unknown("foo".into())),
            instruction!(Unknown("bar".into()), variable("baz"), number(1)),
        ];
    }

    #[test]
    fn test_keyword_as_value() {
        assert_ast![
            "
            print noop
            print stop
            print print
            print label:
            ",
            instruction!(Print {
                value: variable("noop")
            }),
            instruction!(Print {
                value: variable("stop")
            }),
            instruction!(Print {
                value: variable("print")
            }),
            instruction!(Print {
                value: variable("label:")
            }),
        ];
    }

    #[test]
    fn test_label() {
        assert_ast![
            r#"
            foo:
            bar"a":
            :a:
            "#,
            Statement::Label("foo".into()),
            Statement::Label(r#"bar"a""#.into()),
            Statement::Label(":a".into()),
        ];
    }

    // instruction-specific tests

    #[test]
    fn test_noop() {
        assert_ast!["noop", instruction!(Noop)];
    }

    #[test]
    fn test_stop() {
        assert_ast!["stop", instruction!(Stop)];
    }

    #[test]
    fn test_print() {
        assert_ast![
            "print foo",
            instruction!(Print {
                value: variable("foo")
            }),
        ];
    }

    #[test]
    fn test_draw() {
        assert_ast![
            "
            draw clear r g b
            draw color r g b a
            draw col color
            draw stroke width
            draw line x1 y1 x2 y2
            draw rect x y width height
            draw lineRect x y width height
            draw poly x y sides radius rotation
            draw linePoly x y sides radius rotation
            draw triangle x1 y1 x2 y2 x3 y3
            draw image x y image size rotation
            draw print x y align
            draw translate x y
            draw scale x y
            draw rotate degrees
            draw reset
            ",
            instruction!(Draw {
                op: DrawOp::Clear,
                x: variable("r"),
                y: variable("g"),
                p1: variable("b"),
                p2: Value::None,
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Color,
                x: variable("r"),
                y: variable("g"),
                p1: variable("b"),
                p2: variable("a"),
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Col,
                x: variable("color"),
                y: Value::None,
                p1: Value::None,
                p2: Value::None,
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Stroke,
                x: variable("width"),
                y: Value::None,
                p1: Value::None,
                p2: Value::None,
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Line,
                x: variable("x1"),
                y: variable("y1"),
                p1: variable("x2"),
                p2: variable("y2"),
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Rect,
                x: variable("x"),
                y: variable("y"),
                p1: variable("width"),
                p2: variable("height"),
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::LineRect,
                x: variable("x"),
                y: variable("y"),
                p1: variable("width"),
                p2: variable("height"),
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Poly,
                x: variable("x"),
                y: variable("y"),
                p1: variable("sides"),
                p2: variable("radius"),
                p3: variable("rotation"),
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::LinePoly,
                x: variable("x"),
                y: variable("y"),
                p1: variable("sides"),
                p2: variable("radius"),
                p3: variable("rotation"),
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Triangle,
                x: variable("x1"),
                y: variable("y1"),
                p1: variable("x2"),
                p2: variable("y2"),
                p3: variable("x3"),
                p4: variable("y3"),
            }),
            instruction!(Draw {
                op: DrawOp::Image,
                x: variable("x"),
                y: variable("y"),
                p1: variable("image"),
                p2: variable("size"),
                p3: variable("rotation"),
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Print,
                x: variable("x"),
                y: variable("y"),
                p1: variable("align"),
                p2: Value::None,
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Translate,
                x: variable("x"),
                y: variable("y"),
                p1: Value::None,
                p2: Value::None,
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Scale,
                x: variable("x"),
                y: variable("y"),
                p1: Value::None,
                p2: Value::None,
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Rotate,
                x: variable("degrees"),
                y: Value::None,
                p1: Value::None,
                p2: Value::None,
                p3: Value::None,
                p4: Value::None,
            }),
            instruction!(Draw {
                op: DrawOp::Reset,
                x: Value::None,
                y: Value::None,
                p1: Value::None,
                p2: Value::None,
                p3: Value::None,
                p4: Value::None,
            }),
        ];
    }
}
