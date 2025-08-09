use alloc::{string::String, vec::Vec};

use crate::types::{ContentType, LAccess};

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

    Equal,
    NotEqual,
    LessThan,
    LessThanEq,
    GreaterThan,
    GreaterThanEq,
    StrictEqual,

    Land,
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
