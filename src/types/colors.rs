use lazy_static::lazy_static;
use rapidhash::fast::RapidHashMap;
use velcro::map_iter;

// https://github.com/Anuken/Arc/blob/071fdffaf220cd57cf971a0ee58db2f321f92ee1/arc-core/src/arc/graphics/Color.java#L11

pub const WHITE: u32 = 0xff_ff_ff_ff;
pub const LIGHT_GRAY: u32 = 0xbf_bf_bf_ff;
pub const GRAY: u32 = 0x7f_7f_7f_ff;
pub const DARK_GRAY: u32 = 0x3f_3f_3f_ff;
pub const BLACK: u32 = 0x00_00_00_ff;
pub const CLEAR: u32 = 0x00_00_00_00;

pub const BLUE: u32 = 0x00_00_ff_ff;
pub const NAVY: u32 = 0x00_00_7f_ff;
pub const ROYAL: u32 = 0x41_69_e1_ff;
pub const SLATE: u32 = 0x70_80_90_ff;
pub const SKY: u32 = 0x87_ce_eb_ff;
pub const CYAN: u32 = 0x00_ff_ff_ff;
pub const TEAL: u32 = 0x00_7f_7f_ff;

pub const GREEN: u32 = 0x00_ff_00_ff;
pub const ACID: u32 = 0x7f_ff_00_ff;
pub const LIME: u32 = 0x32_cd_32_ff;
pub const FOREST: u32 = 0x22_8b_22_ff;
pub const OLIVE: u32 = 0x6b_8e_23_ff;

pub const YELLOW: u32 = 0xff_ff_00_ff;
pub const GOLD: u32 = 0xff_d7_00_ff;
pub const GOLDENROD: u32 = 0xda_a5_20_ff;
pub const ORANGE: u32 = 0xff_a5_00_ff;

pub const BROWN: u32 = 0x8b_45_13_ff;
pub const TAN: u32 = 0xd2_b4_8c_ff;
pub const BRICK: u32 = 0xb2_22_22_ff;

pub const RED: u32 = 0xff_00_00_ff;
pub const SCARLET: u32 = 0xff_34_1c_ff;
pub const CRIMSON: u32 = 0xdc_14_3c_ff;
pub const CORAL: u32 = 0xff_7f_50_ff;
pub const SALMON: u32 = 0xfa_80_72_ff;
pub const PINK: u32 = 0xff_69_b4_ff;
pub const MAGENTA: u32 = 0xff_00_ff_ff;

pub const PURPLE: u32 = 0xa0_20_f0_ff;
pub const VIOLET: u32 = 0xee_82_ee_ff;
pub const MAROON: u32 = 0xb0_30_60_ff;

pub const TEAM_DERELICT: u32 = 0x4d_4e_58_ff;
pub const TEAM_SHARDED: u32 = 0xff_d3_7f_ff;
pub const TEAM_CRUX: u32 = 0xf2_55_55_ff;
pub const TEAM_MALIS: u32 = 0xa2_7c_e5_ff;
pub const TEAM_GREEN: u32 = 0x54_d6_7d_ff;
pub const TEAM_BLUE: u32 = 0x6c_87_fd_ff;
pub const TEAM_NEOPLASTIC: u32 = 0xe0_54_38_ff;

pub const TEAM_DERELICT_F64: f64 = rgba8888_to_double_bits(TEAM_DERELICT);
pub const TEAM_SHARDED_F64: f64 = rgba8888_to_double_bits(TEAM_SHARDED);
pub const TEAM_CRUX_F64: f64 = rgba8888_to_double_bits(TEAM_CRUX);
pub const TEAM_MALIS_F64: f64 = rgba8888_to_double_bits(TEAM_MALIS);
pub const TEAM_GREEN_F64: f64 = rgba8888_to_double_bits(TEAM_GREEN);
pub const TEAM_BLUE_F64: f64 = rgba8888_to_double_bits(TEAM_BLUE);
pub const TEAM_NEOPLASTIC_F64: f64 = rgba8888_to_double_bits(TEAM_NEOPLASTIC);

lazy_static! {
    // https://github.com/Anuken/Arc/blob/071fdffaf220cd57cf971a0ee58db2f321f92ee1/arc-core/src/arc/graphics/Colors.java#L53
    pub static ref COLORS: RapidHashMap<String, f64> = map_iter! {
        "CLEAR": CLEAR,
        "BLACK": BLACK,

        "WHITE": WHITE,
        "LIGHT_GRAY": LIGHT_GRAY,
        "GRAY": GRAY,
        "DARK_GRAY": DARK_GRAY,
        "LIGHT_GREY": LIGHT_GRAY,
        "GREY": GRAY,
        "DARK_GREY": DARK_GRAY,

        "BLUE": ROYAL,
        "NAVY": NAVY,
        "ROYAL": ROYAL,
        "SLATE": SLATE,
        "SKY": SKY,
        "CYAN": CYAN,
        "TEAL": TEAL,

        "GREEN": 0x38_d6_67_ffu32,
        "ACID": ACID,
        "LIME": LIME,
        "FOREST": FOREST,
        "OLIVE": OLIVE,

        "YELLOW": YELLOW,
        "GOLD": GOLD,
        "GOLDENROD": GOLDENROD,
        "ORANGE": ORANGE,

        "BROWN": BROWN,
        "TAN": TAN,
        "BRICK": BRICK,

        "RED": 0xe5_54_54_ffu32,
        "SCARLET": SCARLET,
        "CRIMSON": CRIMSON,
        "CORAL": CORAL,
        "SALMON": SALMON,
        "PINK": PINK,
        "MAGENTA": MAGENTA,

        "PURPLE": PURPLE,
        "VIOLET": VIOLET,
        "MAROON": MAROON,
    }
    .flat_map(|(k, v)| [(k.to_lowercase(), v), (k.to_string(), v)])
    .map(|(k, v)| (k, rgba8888_to_double_bits(v)))
    .collect();
}

pub const fn f32_to_double_bits(r: f32, g: f32, b: f32, a: f32) -> f64 {
    to_double_bits(
        (r * 255.) as i32,
        (g * 255.) as i32,
        (b * 255.) as i32,
        (a * 255.) as i32,
    )
}

pub const fn to_double_bits(r: i32, g: i32, b: i32, a: i32) -> f64 {
    rgba8888_to_double_bits(((r << 24) | (g << 16) | (b << 8) | a) as u32)
}

pub const fn rgba8888_to_double_bits(value: u32) -> f64 {
    f64::from_bits(value as u64)
}

pub const fn from_double_bits(value: f64) -> (u8, u8, u8, u8) {
    let value = value.to_bits();
    (
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    )
}

pub const fn f64_from_double_bits(value: f64) -> (f64, f64, f64, f64) {
    let (r, g, b, a) = from_double_bits(value);
    (
        (r as f64) / 255.,
        (g as f64) / 255.,
        (b as f64) / 255.,
        (a as f64) / 255.,
    )
}
