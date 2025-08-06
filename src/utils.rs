use widestring::{U16Str, U16String};

macro_rules! u16format {
    ($($arg:tt)*) => {
        {
            use std::fmt::Write;
            use widestring::U16String;
            let mut s = U16String::new();
            write!(s, $($arg)*).unwrap();
            s
        }
    };
}

pub(crate) use u16format;

pub(crate) fn leak_u16string(s: U16String) -> &'static mut U16Str {
    let slice = s.into_vec().leak();
    U16Str::from_slice_mut(slice)
}
