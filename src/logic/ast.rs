use std::error::Error;

use lazy_static::lazy_static;
use num_traits::AsPrimitive;
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Label(String),
    /// `1` contains any extra unused arguments.
    Instruction(Instruction, Vec<Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Instruction {
    // input/output
    Read {
        result: Value,
        target: Value,
        address: Value,
    },
    Write {
        value: Value,
        target: Value,
        address: Value,
    },
    Draw {
        op: DrawOp,
        x: Value,
        y: Value,
        p1: Value,
        p2: Value,
        p3: Value,
        p4: Value,
    },
    Print {
        value: Value,
    },
    PrintChar {
        value: Value,
    },
    Format {
        value: Value,
    },
    // block control
    DrawFlush {
        target: Value,
    },
    PrintFlush {
        target: Value,
    },
    GetLink {
        result: Value,
        index: Value,
    },
    Control {
        control: LAccess,
        target: Value,
        p1: Value,
        p2: Value,
        p3: Value,
    },
    Sensor {
        result: Value,
        target: Value,
        sensor: Value,
    },
    // operations
    Set {
        to: Value,
        from: Value,
    },
    Op {
        op: LogicOp,
        result: Value,
        x: Value,
        y: Value,
    },
    Select {
        result: Value,
        op: ConditionOp,
        x: Value,
        y: Value,
        if_true: Value,
        if_false: Value,
    },
    Lookup {
        content_type: ContentType,
        result: Value,
        id: Value,
    },
    PackColor {
        result: Value,
        r: Value,
        g: Value,
        b: Value,
        a: Value,
    },
    UnpackColor {
        r: Value,
        g: Value,
        b: Value,
        a: Value,
        value: Value,
    },
    // flow control
    Noop,
    Wait {
        value: Value,
    },
    Stop,
    End,
    Jump {
        target: Value,
        op: ConditionOp,
        x: Value,
        y: Value,
    },
    // privileged
    GetBlock {
        layer: TileLayer,
        result: Value,
        x: Value,
        y: Value,
    },
    SetRate {
        value: Value,
    },
    // unknown
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawOp {
    Clear,
    Color,
    Col,
    Stroke,
    Line,
    Rect,
    LineRect,
    Poly,
    LinePoly,
    Triangle,
    Image,
    Print,
    Translate,
    Scale,
    Rotate,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConditionOp {
    Equal,
    NotEqual,
    LessThan,
    LessThanEq,
    GreaterThan,
    GreaterThanEq,
    StrictEqual,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogicOp {
    Add,
    Sub,
    Mul,
    Div,
    Idiv,
    Mod,
    Emod,
    Pow,

    Land,
    Condition(ConditionOp),

    Shl,
    Shr,
    Ushr,
    Or,
    And,
    Xor,
    Not,

    Max,
    Min,
    Angle,
    AngleDiff,
    Len,
    Noise,
    Abs,
    Sign,
    Log,
    Logn,
    Log10,
    Floor,
    Ceil,
    Round,
    Sqrt,
    Rand,

    Sin,
    Cos,
    Tan,

    Asin,
    Acos,
    Atan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileLayer {
    Floor,
    Ore,
    Block,
    Building,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Variable(String),
    String(String),
    Number(f64),
    /// Placeholder for unused arguments, eg. `Jump.x` and `Jump.y` with `ConditionOp::Always`.
    None,
}

lazy_static! {
    static ref NUMBER_RE: Regex = Regex::new(
        r"(?x)
            ^
            (?<sign>[+-])?
            (?:
                # decimal
                (?<dec_int> [+-]?[0-9]+ )?        # integer (optional)
                \. (?<dec_frac> \+?[0-9]+ | -0+ ) # fraction
                |
                # integer or scientific notation
                (?<sci_int> [+-]?[0-9]+ )             # integer
                (?: [eE] (?<sci_exp> [+-]?[0-9]+ ) )? # exponent (optional)
            )
            [fF.]?
            $
        "
    )
    .unwrap();
}

// https://github.com/Anuken/Arc/blob/071fdffaf220cd57cf971a0ee58db2f321f92ee1/arc-core/src/arc/util/Strings.java#L495
pub(super) fn parse_number(n: &str) -> Result<f64, Box<dyn Error>> {
    // this should never fail, unless we forgot to update one of the regexes
    let caps = NUMBER_RE.captures(n).expect("failed to match regex");

    let sign = match caps.name("sign") {
        Some(m) if m.as_str() == "-" => -1.,
        _ => 1.,
    };

    Ok(match caps.name("dec_frac") {
        // decimal
        Some(dec_frac) => {
            let whole = match caps.name("dec_int") {
                Some(m) => m.as_str().parse()?,
                None => 0i64,
            } as f64;

            let dec = dec_frac.as_str().parse::<i64>()? as f64;

            whole + (dec / 10f64.powf(dec_frac.len() as f64)).copysign(whole)
        }

        None => {
            let whole = caps["sci_int"].parse::<i64>()? as f64;

            match caps.name("sci_exp") {
                // scientific notation
                Some(sci_exp) => {
                    let power = sci_exp.as_str().parse::<i64>()? as f64;

                    whole * 10f64.powf(power)
                }

                // integer
                None => whole,
            }
        }
    } * sign)
}

pub(super) fn number_to_value<T, E>(n: &str, res: Result<T, E>) -> Value
where
    T: AsPrimitive<f64>,
{
    match res {
        Ok(value) => Value::Number(value.as_()),
        Err(_) => Value::Variable(n.into()),
    }
}

macro_rules! optional_args {
    ($($typ:ident)::+ { $($name:ident$(: $value:expr)?),+ ; $($extra:ident),+ $(,)? }) => {
        $($typ)::+ {
            $($name$(: $value)?),+ ,
            $($extra: Value::None),+
        }
    };
}

pub(super) use optional_args;

use crate::types::{ContentType, LAccess};
