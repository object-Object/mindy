use indexmap::IndexMap;
use widestring::{U16Str, U16String};

macro_rules! u16format {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
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

#[cfg(feature = "std")]
type BuildHasher = rapidhash::fast::RandomState;
#[cfg(not(feature = "std"))]
type BuildHasher = rapidhash::fast::RapidBuildHasher;

pub(crate) type RapidIndexMap<K, V> = IndexMap<K, V, BuildHasher>;

pub(crate) type RapidHashMap<K, V> = hashbrown::HashMap<K, V, BuildHasher>;
pub(crate) type RapidHashSet<T> = hashbrown::HashSet<T, BuildHasher>;
