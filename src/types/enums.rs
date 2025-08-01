#![allow(deprecated)]

use std::{borrow::Cow, hash::Hash};

use binrw::prelude::*;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[binrw]
#[brw(big)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Team {
    #[brw(magic = 0u8)]
    Derelict,
    #[brw(magic = 1u8)]
    Sharded,
    #[brw(magic = 2u8)]
    Crux,
    #[brw(magic = 3u8)]
    Malis,
    #[brw(magic = 4u8)]
    Green,
    #[brw(magic = 5u8)]
    Blue,
    #[brw(magic = 6u8)]
    Neoplastic,
    Unknown(u8),
}

impl Team {
    pub fn base_teams() -> impl Iterator<Item = Self> {
        (0u8..=6u8).map(Self::from_id)
    }

    pub fn from_id(id: u8) -> Self {
        match id {
            0 => Self::Derelict,
            1 => Self::Sharded,
            2 => Self::Crux,
            3 => Self::Malis,
            4 => Self::Green,
            5 => Self::Blue,
            6 => Self::Neoplastic,
            _ => Self::Unknown(id),
        }
    }

    pub fn name(&self) -> Cow<'_, str> {
        match self {
            Self::Derelict => Cow::from("Derelict"),
            Self::Sharded => Cow::from("Sharded"),
            Self::Crux => Cow::from("Crux"),
            Self::Malis => Cow::from("Malis"),
            Self::Green => Cow::from("Green"),
            Self::Blue => Cow::from("Blue"),
            Self::Neoplastic => Cow::from("Neoplastic"),
            // TODO: this probably shouldn't need to allocate
            Self::Unknown(i) => Cow::Owned(format!("team#{i}")),
        }
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
