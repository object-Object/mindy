#![allow(deprecated)]

use std::hash::Hash;

use binrw::prelude::*;
use lazy_static::lazy_static;
use strum_macros::{AsRefStr, IntoStaticStr, VariantArray};
use velcro::vec;

use super::colors;

#[binrw]
#[brw(big, repr = i8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    Item,
    Block,
    #[deprecated]
    Mech,
    Bullet,
    Liquid,
    Status,
    Unit,
    Weather,
    #[deprecated]
    Effect,
    Sector,
    #[deprecated]
    Loadout,
    #[deprecated]
    TypeID,
    Error,
    Planet,
    #[deprecated]
    Ammo,
    Team,
    UnitCommand,
    UnitStance,
}

#[binrw]
#[brw(big, repr = i16)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    AsRefStr,
    IntoStaticStr,
    VariantArray,
    strum_macros::Display,
)]
#[strum(serialize_all = "camelCase")]
pub enum LAccess {
    TotalItems,
    FirstItem,
    TotalLiquids,
    TotalPower,
    ItemCapacity,
    LiquidCapacity,
    PowerCapacity,
    PowerNetStored,
    PowerNetCapacity,
    PowerNetIn,
    PowerNetOut,
    Ammo,
    AmmoCapacity,
    CurrentAmmoType,
    MemoryCapacity,
    Health,
    MaxHealth,
    Heat,
    Shield,
    Armor,
    Efficiency,
    Progress,
    Timescale,
    Rotation,
    X,
    Y,
    VelocityX,
    VelocityY,
    ShootX,
    ShootY,
    CameraX,
    CameraY,
    CameraWidth,
    CameraHeight,
    DisplayWidth,
    DisplayHeight,
    BufferSize,
    Operations,
    Size,
    Solid,
    Dead,
    Range,
    Shooting,
    Boosting,
    MineX,
    MineY,
    Mining,
    Speed,
    Team,
    Type,
    Flag,
    Controlled,
    Controller,
    Name,
    PayloadCount,
    PayloadType,
    TotalPayload,
    PayloadCapacity,
    Id,
    Enabled,
    Shoot,
    Shootp,
    Config,
    Color,
}

lazy_static! {
    static ref TEAM_NAMES: Vec<&'static str> = vec![
        "derelict",
        "sharded",
        "crux",
        "malis",
        "green",
        "blue",
        "neoplastic",
        ..(Team::BASE_TEAMS.len()..256).map(|i| -> &'static str { format!("team#{i}").leak() }),
    ];
}

const TEAM_COLORS: &[f64] = &[
    colors::TEAM_DERELICT_F64,
    colors::TEAM_SHARDED_F64,
    colors::TEAM_CRUX_F64,
    colors::TEAM_MALIS_F64,
    colors::TEAM_GREEN_F64,
    colors::TEAM_BLUE_F64,
    colors::TEAM_NEOPLASTIC_F64,
    // TODO: unnamed teams
];

#[binrw]
#[brw(big)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Team(pub u8);

impl Team {
    pub const DERELICT: Self = Self(0);
    pub const SHARDED: Self = Self(1);
    pub const CRUX: Self = Self(2);
    pub const MALIS: Self = Self(3);
    pub const GREEN: Self = Self(4);
    pub const BLUE: Self = Self(5);
    pub const NEOPLASTIC: Self = Self(6);

    pub const BASE_TEAMS: &[Self] = &[
        Self::DERELICT,
        Self::SHARDED,
        Self::CRUX,
        Self::MALIS,
        Self::GREEN,
        Self::BLUE,
        Self::NEOPLASTIC,
    ];

    pub fn name(&self) -> &'static str {
        TEAM_NAMES[self.0 as usize]
    }

    pub fn color(&self) -> f64 {
        TEAM_COLORS.get(self.0 as usize).copied().unwrap_or(0.)
    }
}

#[binrw]
#[brw(big, repr = u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnitCommand {
    Move,
    Repair,
    Rebuild,
    Assist,
    Mine,
    Boost,
    EnterPayload,
    LoadUnits,
    LoadBlocks,
    UnloadPayload,
    LoopPayload,
}
