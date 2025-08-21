use js_sys::JsString;
use mindy::types::PackedPoint2;
use wasm_bindgen::JsValue;
use widestring::{U16Str, U16String};

pub const MAX_DELTA: f64 = 6.;

pub fn pack_point(position: PackedPoint2) -> u32 {
    ((position.y as u32) << 16) | (position.x as u32)
}

pub fn unpack_point(position: u32) -> PackedPoint2 {
    PackedPoint2 {
        x: position as i16,
        y: (position >> 16) as i16,
    }
}

pub fn fps_to_delta(fps: f64) -> f64 {
    // nominal 60 fps
    // 60 / 60 -> 1
    // 60 / 120 -> 0.5
    // 60 / 30 -> 2
    (60. / fps).min(MAX_DELTA)
}

pub fn delta_to_time(delta: f64) -> f64 {
    // 60 ticks per second
    delta / 60.
}

pub fn on_building_change(
    f: &js_sys::Function,
    position: PackedPoint2,
    building_type: &str,
    value: impl Into<JsValue>,
) {
    f.call3(
        &JsValue::NULL,
        &pack_point(position).into(),
        &building_type.into(),
        &value.into(),
    )
    .unwrap();
}

pub fn u16string_from_js(s: &JsString) -> U16String {
    let mut value = U16String::new();
    value.as_mut_vec().extend(s.iter());
    value
}

pub fn u16str_to_js(s: &U16Str) -> JsString {
    JsString::from_char_code(s.as_slice())
}
