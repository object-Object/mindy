use std::error::Error;

use lazy_static::lazy_static;
use num_traits::AsPrimitive;
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Instruction(String, Vec<Value>),
    Label(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Variable(String),
    String(String),
    Number(f64),
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

    match caps.name("dec_frac") {
        // decimal
        Some(dec_frac) => {
            let whole = match caps.name("dec_int") {
                Some(m) => m.as_str().parse()?,
                None => 0i64,
            } as f64;

            let dec = dec_frac.as_str().parse::<i64>()? as f64;

            Ok((whole + (dec / 10f64.powf(dec_frac.len() as f64)).copysign(whole)) * sign)
        }

        None => {
            let whole = caps["sci_int"].parse::<i64>()? as f64;

            match caps.name("sci_exp") {
                // scientific notation
                Some(sci_exp) => {
                    let power = sci_exp.as_str().parse::<i64>()? as f64;

                    Ok(whole * 10f64.powf(power) * sign)
                }

                // integer
                None => Ok(whole),
            }
        }
    }
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
